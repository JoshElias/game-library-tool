#!/usr/bin/env python3
"""Safely apply one reviewed SteamGridDB art set to Lutris and Steam.

The helper is intentionally conservative. User-selected art in either launcher
skips the whole game. Art created by Lutris for a new Steam shortcut is only
replaceable when a trusted pre-shortcut baseline proves it was absent before.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import struct
import subprocess
import sys
import tempfile
import time
import unicodedata
import urllib.error
import urllib.parse
import urllib.request
import warnings

API_BASE = "https://www.steamgriddb.com/api/v2"
MAX_IMAGE_BYTES = 25 * 1024 * 1024
MAX_IMAGE_PIXELS = 50_000_000
MAX_VDF_BYTES = 16 * 1024 * 1024
MAX_VDF_STRING = 64 * 1024
MAX_VDF_DEPTH = 16
MAX_VDF_ENTRIES = 50_000
MAX_JSON_BYTES = 4 * 1024 * 1024
MIME_FORMATS = {
    "image/png": "PNG",
    "image/jpeg": "JPEG",
    "image/webp": "WEBP",
    "image/vnd.microsoft.icon": "ICO",
    "image/x-icon": "ICO",
}
ART_REQUESTS = {
    "landscape": ("grids", {"dimensions": "920x430", "types": "static", "limit": "100"}, ""),
    "capsule": ("grids", {"dimensions": "600x900", "types": "static", "limit": "100"}, "p"),
    "hero": ("heroes", {"dimensions": "3840x1240,1920x620,1600x650", "types": "static", "limit": "100"}, "_hero"),
    "logo": ("logos", {"styles": "official,white,custom,black", "types": "static", "limit": "100"}, "_logo"),
    "icon": ("icons", {"styles": "official,custom", "dimensions": "1024,768,512,256", "types": "static", "limit": "100"}, "_icon"),
}
LUTRIS_DERIVATIVES = {
    "banner": ("landscape", (184, 69)),
    "cover": ("capsule", (264, 352)),
    "icon": ("icon", (128, 128)),
}
CUSTOM_FLAGS = {
    "banner": "has_custom_banner",
    "cover": "has_custom_coverart_big",
    "icon": "has_custom_icon",
}
ART_ID_ARGUMENTS = {
    "landscape": "landscape_art_id",
    "capsule": "capsule_art_id",
    "hero": "hero_art_id",
    "logo": "logo_art_id",
    "icon": "icon_art_id",
}


class ArtworkError(RuntimeError):
    pass


@dataclass(frozen=True)
class LutrisTarget:
    game_id: str
    title: str
    slug: str
    banner: Path
    cover: Path
    icon: Path
    custom_banner: bool
    custom_cover: bool
    custom_icon: bool

    @property
    def custom_types(self) -> tuple[str, ...]:
        result = []
        if self.custom_banner:
            result.append("banner")
        if self.custom_cover:
            result.append("cover")
        if self.custom_icon:
            result.append("icon")
        return tuple(result)

    @property
    def destinations(self) -> dict[str, Path]:
        return {"banner": self.banner, "cover": self.cover, "icon": self.icon}

    @property
    def media_candidates(self) -> tuple[Path, ...]:
        return (
            self.banner.with_suffix(".jpg"),
            self.banner,
            self.cover.with_suffix(".jpg"),
            self.cover,
            self.icon,
        )


@dataclass(frozen=True)
class ExistingArt:
    protected: tuple[Path, ...]
    replaceable_defaults: tuple[Path, ...]
    lutris_custom_types: tuple[str, ...]


class LutrisBackend:
    """Narrow adapter over the installed Lutris application's metadata API."""

    def __init__(self) -> None:
        try:
            from lutris import settings
            from lutris.database import games
        except (ImportError, ValueError, RuntimeError) as exc:
            raise ArtworkError(f"installed Lutris metadata API is unavailable: {exc}") from exc
        self.settings = settings
        self.games = games

    def get_record(self, game_id: str) -> dict[str, object] | None:
        record = self.games.get_game_by_field(game_id, "id")
        return record if isinstance(record, dict) and record else None

    def media_roots(self) -> tuple[Path, Path, Path]:
        return Path(self.settings.BANNER_PATH), Path(self.settings.COVERART_PATH), Path(self.settings.ICON_PATH)

    def set_custom_flags(self, game_id: str, flags: dict[str, bool]) -> None:
        params: dict[str, object] = {"id": game_id}
        for kind, field in CUSTOM_FLAGS.items():
            params[field] = 1 if flags[kind] else 0
        updated = self.games.update_existing(**params)
        if str(updated) != str(game_id):
            raise ArtworkError(f"Lutris refused to update exact game ID {game_id}")
        record = self.get_record(game_id)
        if record is None:
            raise ArtworkError(f"Lutris game ID {game_id} disappeared after metadata update")
        for kind, field in CUSTOM_FLAGS.items():
            if bool(record.get(field)) != flags[kind]:
                raise ArtworkError(f"Lutris did not retain the {kind} custom-art flag")


def resolve_lutris_target(game_id: str, backend: object) -> LutrisTarget:
    record = backend.get_record(str(game_id))
    if not record:
        raise ArtworkError(f"no Lutris game record found for exact ID {game_id}")
    if not bool(record.get("installed")) or not record.get("configpath"):
        raise ArtworkError(f"Lutris game ID {game_id} is not installed")
    slug = str(record.get("slug") or "")
    if not re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9_-]*", slug):
        raise ArtworkError(f"unsafe or empty Lutris slug for game ID {game_id}")
    title = str(record.get("name") or "").strip()
    if not title:
        raise ArtworkError(f"Lutris game ID {game_id} has no title")
    banner_root, cover_root, icon_root = backend.media_roots()
    return LutrisTarget(
        game_id=str(game_id),
        title=title,
        slug=slug,
        banner=banner_root / f"{slug}.png",
        cover=cover_root / f"{slug}.png",
        icon=icon_root / f"lutris_{slug}.png",
        custom_banner=bool(record.get("has_custom_banner")),
        custom_cover=bool(record.get("has_custom_coverart_big")),
        custom_icon=bool(record.get("has_custom_icon")),
    )


def _read_cstring(data: bytes, offset: int) -> tuple[str, int]:
    try:
        end = data.index(0, offset)
    except ValueError as exc:
        raise ArtworkError("malformed shortcuts.vdf string") from exc
    if end - offset > MAX_VDF_STRING:
        raise ArtworkError("shortcuts.vdf string exceeds the size limit")
    return data[offset:end].decode("utf-8", "replace"), end + 1


def _read_vdf_object(data: bytes, offset: int, depth: int = 0, counter: list[int] | None = None) -> tuple[dict[str, object], int]:
    if depth > MAX_VDF_DEPTH:
        raise ArtworkError("shortcuts.vdf nesting exceeds the depth limit")
    if counter is None:
        counter = [0]
    result: dict[str, object] = {}
    while offset < len(data):
        value_type = data[offset]
        offset += 1
        if value_type == 8:
            return result, offset
        counter[0] += 1
        if counter[0] > MAX_VDF_ENTRIES:
            raise ArtworkError("shortcuts.vdf exceeds the entry limit")
        key, offset = _read_cstring(data, offset)
        if key in result:
            raise ArtworkError(f"shortcuts.vdf contains duplicate field {key!r}")
        if value_type == 0:
            value, offset = _read_vdf_object(data, offset, depth + 1, counter)
        elif value_type == 1:
            value, offset = _read_cstring(data, offset)
        elif value_type == 2:
            if offset + 4 > len(data):
                raise ArtworkError("truncated shortcuts.vdf integer")
            value = struct.unpack_from("<i", data, offset)[0]
            offset += 4
        else:
            raise ArtworkError(f"unsupported shortcuts.vdf value type {value_type}")
        result[key] = value
    raise ArtworkError("unterminated shortcuts.vdf object")


def read_shortcuts(path: Path) -> list[dict[str, object]]:
    size = path.stat().st_size
    if size > MAX_VDF_BYTES:
        raise ArtworkError(f"shortcuts.vdf exceeds the {MAX_VDF_BYTES}-byte limit")
    data = path.read_bytes()
    root, offset = _read_vdf_object(data, 0)
    if offset != size:
        raise ArtworkError(f"trailing data in {path}")
    shortcuts = root.get("shortcuts")
    if not isinstance(shortcuts, dict):
        raise ArtworkError(f"missing shortcuts object in {path}")
    return [entry for entry in shortcuts.values() if isinstance(entry, dict)]


def _casefolded(entry: dict[str, object]) -> dict[str, object]:
    return {key.casefold(): value for key, value in entry.items()}


def lutris_shortcuts(path: Path, lutris_id: str | None) -> list[dict[str, object]]:
    matches = []
    wanted = None if lutris_id is None else f"lutris:rungameid/{lutris_id}".casefold()
    for raw in read_shortcuts(path):
        entry = _casefolded(raw)
        launch = f"{entry.get('exe', '')} {entry.get('launchoptions', '')}".casefold()
        if "lutris" not in launch:
            continue
        if wanted is not None:
            idx = launch.find(wanted)
            if idx < 0:
                continue
            after = launch[idx + len(wanted) : idx + len(wanted) + 1]
            if after.isdigit():
                continue
        title = str(entry.get("appname", "")).strip()
        appid = entry.get("appid")
        if not title or not isinstance(appid, int):
            raise ArtworkError(f"Lutris shortcut in {path} lacks appname or appid")
        matches.append({"title": title, "appid": appid & 0xFFFFFFFF, "raw": raw})
    return matches


def find_shortcut_files(explicit: Path | None) -> list[Path]:
    if explicit is not None:
        return [explicit.expanduser()]
    home = Path.home()
    roots = [home / ".local/share/Steam/userdata", home / ".steam/steam/userdata"]
    found: dict[Path, Path] = {}
    for root in roots:
        if root.is_dir():
            for path in root.glob("*/config/shortcuts.vdf"):
                found[path.resolve()] = path
    return sorted(found.values())


def _steam_pattern(appid: int | None = None) -> re.Pattern[str]:
    prefix = str(appid) if appid is not None else r"\d+"
    return re.compile(rf"^{prefix}(?:p|_hero|_logo|_icon)?\.(?:png|jpe?g|webp|ico)$", re.IGNORECASE)


def custom_art(path: Path, appid: int) -> list[Path]:
    grid = path.parent / "grid"
    if not grid.is_dir():
        return []
    pattern = _steam_pattern(appid)
    return sorted(candidate for candidate in grid.iterdir() if candidate.is_file() and pattern.fullmatch(candidate.name))


def normalize_title(value: str) -> str:
    normalized = unicodedata.normalize("NFKD", value).casefold()
    return "".join(character for character in normalized if character.isalnum())


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def _ensure_owned_regular(path: Path, *, may_not_exist: bool = False) -> None:
    if not path.exists():
        if may_not_exist:
            return
        raise ArtworkError(f"required path is absent: {path}")
    if path.is_symlink() or not path.is_file():
        raise ArtworkError(f"path is not a regular non-symlink file: {path}")
    if path.stat().st_uid != os.getuid():
        raise ArtworkError(f"path is not owned by the desktop user: {path}")


def _ensure_owned_directory(path: Path, *, create: bool = False, mode: int = 0o700) -> None:
    if create:
        path.mkdir(parents=True, exist_ok=True, mode=mode)
    if path.is_symlink() or not path.is_dir():
        raise ArtworkError(f"path is not a real directory: {path}")
    if path.stat().st_uid != os.getuid():
        raise ArtworkError(f"directory is not owned by the desktop user: {path}")


def _runtime_baseline_path(path: Path) -> Path:
    value = os.environ.get("XDG_RUNTIME_DIR")
    if not value:
        raise ArtworkError("XDG_RUNTIME_DIR is required for a trusted Steam baseline")
    runtime = Path(value)
    if not runtime.is_absolute():
        raise ArtworkError("XDG_RUNTIME_DIR must be absolute")
    _ensure_owned_directory(runtime)
    candidate = path.expanduser().absolute()
    resolved_runtime = runtime.resolve()
    resolved_candidate = candidate.resolve(strict=False)
    if not resolved_candidate.is_relative_to(resolved_runtime) or resolved_candidate == resolved_runtime:
        raise ArtworkError("baseline must remain beneath XDG_RUNTIME_DIR")
    current = resolved_runtime
    for part in candidate.parent.relative_to(runtime).parts:
        current /= part
        if os.path.lexists(current) and current.is_symlink():
            raise ArtworkError(f"baseline path traverses a symlink: {current}")
    return candidate


def capture_baseline(shortcuts: Path, baseline: Path) -> None:
    baseline = _runtime_baseline_path(baseline)
    _ensure_owned_regular(shortcuts)
    _ensure_owned_directory(baseline.parent, create=True)
    if baseline.exists():
        raise ArtworkError(f"baseline already exists: {baseline}")
    grid = shortcuts.parent / "grid"
    files: dict[str, str] = {}
    if grid.exists():
        _ensure_owned_directory(grid)
        for path in grid.iterdir():
            if path.is_file() and not path.is_symlink() and _steam_pattern().fullmatch(path.name):
                files[path.name] = sha256(path)
    shortcut_stat = shortcuts.stat()
    payload = {
        "version": 2,
        "uid": os.getuid(),
        "shortcuts": str(shortcuts.resolve()),
        "shortcuts_sha256_before": sha256(shortcuts),
        "shortcuts_size_before": shortcut_stat.st_size,
        "shortcuts_mtime_ns_before": shortcut_stat.st_mtime_ns,
        "grid": str(grid.resolve()),
        "captured_ns": time.time_ns(),
        "nonce": secrets.token_hex(16),
        "files": files,
    }
    descriptor = os.open(baseline, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w") as output:
        json.dump(payload, output, sort_keys=True)
        output.write("\n")
        output.flush()
        os.fsync(output.fileno())
    _fsync_directory(baseline.parent)
    print(f"BASELINE shortcuts={shortcuts} grid={grid} object={baseline} entries={len(files)}")


def read_baseline(path: Path, shortcuts: Path) -> dict[str, str]:
    path = _runtime_baseline_path(path)
    _ensure_owned_regular(path)
    if path.stat().st_mode & 0o077:
        raise ArtworkError(f"baseline must not grant group/other access: {path}")
    try:
        payload = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        raise ArtworkError(f"invalid baseline: {path}") from exc
    expected_grid = str((shortcuts.parent / "grid").resolve())
    captured_ns = payload.get("captured_ns")
    if (
        payload.get("version") != 2
        or payload.get("uid") != os.getuid()
        or payload.get("shortcuts") != str(shortcuts.resolve())
        or payload.get("grid") != expected_grid
        or not isinstance(captured_ns, int)
        or captured_ns > time.time_ns() + 60_000_000_000
        or time.time_ns() - captured_ns > 24 * 60 * 60 * 1_000_000_000
        or not re.fullmatch(r"[0-9a-f]{32}", str(payload.get("nonce", "")))
        or not re.fullmatch(r"[0-9a-f]{64}", str(payload.get("shortcuts_sha256_before", "")))
        or not isinstance(payload.get("shortcuts_size_before"), int)
        or not isinstance(payload.get("shortcuts_mtime_ns_before"), int)
    ):
        raise ArtworkError("baseline is stale, malformed, or does not match the exact Steam account")
    files = payload.get("files")
    if not isinstance(files, dict) or not all(isinstance(k, str) and isinstance(v, str) for k, v in files.items()):
        raise ArtworkError("baseline has invalid file metadata")
    return files


def classify_existing(shortcuts: Path, appid: int, target: LutrisTarget, baseline: Path | None) -> ExistingArt:
    existing = custom_art(shortcuts, appid)
    protected: list[Path] = []
    replaceable: list[Path] = []
    if existing:
        if baseline is None:
            protected.extend(existing)
        else:
            prior = read_baseline(baseline, shortcuts)
            for path in existing:
                (protected if path.name in prior else replaceable).append(path)
    return ExistingArt(tuple(protected), tuple(replaceable), target.custom_types)


class SteamGridDB:
    def __init__(self, api_key: str, base: str = API_BASE):
        self.api_key = api_key
        self.base = base.rstrip("/")

    def _json(self, path: str, query: dict[str, str] | None = None) -> object:
        url = f"{self.base}/{path.lstrip('/')}"
        if query:
            url += "?" + urllib.parse.urlencode(query)
        request = urllib.request.Request(url, headers={"Authorization": f"Bearer {self.api_key}", "User-Agent": "lutris-steam-artwork/2"})
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                length = response.headers.get("Content-Length")
                if length is not None and int(length) > MAX_JSON_BYTES:
                    raise ArtworkError("SteamGridDB metadata response exceeds the byte limit")
                raw = response.read(MAX_JSON_BYTES + 1)
                if len(raw) > MAX_JSON_BYTES:
                    raise ArtworkError("SteamGridDB metadata response exceeds the byte limit")
                payload = json.loads(raw)
        except (urllib.error.URLError, TimeoutError, json.JSONDecodeError, OSError, ValueError) as exc:
            if isinstance(exc, ArtworkError):
                raise
            raise ArtworkError(f"SteamGridDB request failed for {path}: {exc}") from exc
        if not isinstance(payload, dict) or not payload.get("success"):
            raise ArtworkError(f"SteamGridDB returned an unsuccessful response for {path}")
        return payload.get("data")

    def game(self, title: str, game_id: int | None) -> tuple[int, str]:
        if game_id is not None:
            return game_id, title
        data = self._json(f"search/autocomplete/{urllib.parse.quote(title, safe='')}")
        if not isinstance(data, list):
            raise ArtworkError("SteamGridDB search returned invalid data")
        exact = [item for item in data if isinstance(item, dict) and normalize_title(str(item.get("name", ""))) == normalize_title(title)]
        if len(exact) == 1 and isinstance(exact[0].get("id"), int):
            return int(exact[0]["id"]), str(exact[0].get("name", title))
        candidates = [f"{item.get('id')}:{item.get('name')}" for item in data[:8] if isinstance(item, dict)]
        raise ArtworkError(f"SteamGridDB title match is ambiguous for {title!r}; candidates: {', '.join(candidates) or 'none'}. Review and pass --sgdb-game-id.")

    def art(self, game_id: int, art_type: str, approved_id: int | None = None) -> dict[str, object]:
        endpoint, query, _ = ART_REQUESTS[art_type]
        data = self._json(f"{endpoint}/game/{game_id}", query)
        if not isinstance(data, list) or not data:
            raise ArtworkError(f"SteamGridDB has no {art_type} artwork for game ID {game_id}")
        if approved_id is not None:
            selected = next((item for item in data if isinstance(item, dict) and item.get("id") == approved_id), None)
            if selected is None:
                raise ArtworkError(f"approved {art_type} art ID {approved_id} is no longer available")
        else:
            def rank(item: object) -> tuple[int, int, int]:
                if not isinstance(item, dict):
                    return (-1, -1, -1)
                return (int(item.get("score", 0) or 0), 1 if str(item.get("style", "")) == "official" else 0, int(item.get("width", 0) or 0) * int(item.get("height", 0) or 0))
            selected = max(data, key=rank)
        if not isinstance(selected, dict) or not isinstance(selected.get("id"), int) or not isinstance(selected.get("url"), str):
            raise ArtworkError(f"SteamGridDB returned invalid {art_type} artwork")
        return selected


def read_api_key(path: Path) -> str:
    _ensure_owned_regular(path)
    if path.stat().st_mode & 0o077:
        raise ArtworkError(f"SteamGridDB API key file must not grant group/other access: {path}")
    key = path.read_text().strip()
    if not key:
        raise ArtworkError(f"SteamGridDB API key file is empty: {path}")
    return key


def _safe_image_url(url: str, api_base: str) -> bool:
    parsed = urllib.parse.urlparse(url)
    hostname = (parsed.hostname or "").casefold()
    if parsed.scheme == "https" and (hostname == "steamgriddb.com" or hostname.endswith(".steamgriddb.com")):
        return True
    base = urllib.parse.urlparse(api_base)
    return parsed.scheme == "http" and hostname in {"127.0.0.1", "localhost"} and base.hostname in {"127.0.0.1", "localhost"}


def _pillow():
    try:
        from PIL import Image, ImageOps, UnidentifiedImageError
    except ImportError as exc:
        raise ArtworkError("Pillow is required to validate and derive artwork") from exc
    Image.MAX_IMAGE_PIXELS = MAX_IMAGE_PIXELS
    warnings.filterwarnings("error", category=Image.DecompressionBombWarning)
    return Image, ImageOps, UnidentifiedImageError


def download_png(url: str, target: Path, api_base: str) -> None:
    if not _safe_image_url(url, api_base):
        raise ArtworkError("artwork URL is not an approved HTTPS URL")
    raw = target.with_suffix(".download")
    request = urllib.request.Request(url, headers={"User-Agent": "lutris-steam-artwork/2"})
    written = 0
    declared_format = ""
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            if not _safe_image_url(response.geturl(), api_base):
                raise ArtworkError("artwork download redirected outside HTTPS")
            mime = response.headers.get_content_type().casefold()
            declared_format = MIME_FORMATS.get(mime, "")
            if not declared_format:
                raise ArtworkError(f"unsupported artwork content type: {mime}")
            length = response.headers.get("Content-Length")
            if length is not None and int(length) > MAX_IMAGE_BYTES:
                raise ArtworkError("artwork exceeds the byte limit")
            with raw.open("xb") as output:
                while chunk := response.read(1024 * 1024):
                    written += len(chunk)
                    if written > MAX_IMAGE_BYTES:
                        raise ArtworkError("artwork exceeds the byte limit")
                    output.write(chunk)
        if written == 0:
            raise ArtworkError("artwork download was empty")
        Image, _, UnidentifiedImageError = _pillow()
        try:
            with Image.open(raw) as probe:
                if probe.format != declared_format:
                    raise ArtworkError(f"artwork content does not match declared type: expected {declared_format}, got {probe.format}")
                if int(getattr(probe, "n_frames", 1)) != 1:
                    raise ArtworkError("animated or multi-frame artwork is not allowed")
                probe.verify()
            with Image.open(raw) as source:
                source.load()
                if source.width * source.height > MAX_IMAGE_PIXELS:
                    raise ArtworkError("artwork exceeds the decoded pixel limit")
                converted = source.convert("RGBA" if "A" in source.getbands() else "RGB")
                converted.save(target, "PNG", optimize=True)
        except ArtworkError:
            raise
        except (UnidentifiedImageError, Image.DecompressionBombError, OSError, ValueError) as exc:
            raise ArtworkError(f"artwork failed image validation: {exc}") from exc
    except (urllib.error.URLError, TimeoutError, OSError, ValueError) as exc:
        if isinstance(exc, ArtworkError):
            raise
        raise ArtworkError(f"artwork download failed: {exc}") from exc
    finally:
        raw.unlink(missing_ok=True)


def derive_lutris(source: Path, target: Path, size: tuple[int, int]) -> None:
    Image, ImageOps, UnidentifiedImageError = _pillow()
    try:
        with Image.open(source) as image:
            image.load()
            fitted = ImageOps.fit(image.convert("RGBA"), size, method=Image.Resampling.LANCZOS)
            fitted.save(target, "PNG", optimize=True)
    except (UnidentifiedImageError, OSError, ValueError) as exc:
        raise ArtworkError(f"failed to derive Lutris artwork: {exc}") from exc


def steam_destinations(shortcuts: Path, appid: int) -> dict[str, Path]:
    grid = shortcuts.parent / "grid"
    return {kind: grid / f"{appid}{suffix}.png" for kind, (_, _, suffix) in ART_REQUESTS.items()}


def state_directory(game_id: str, appid: int) -> Path:
    state_home = Path(os.environ.get("XDG_STATE_HOME", str(Path.home() / ".local/state")))
    return state_home / "game-library-artwork" / f"{game_id}-{appid}"


def _old_flags(target: LutrisTarget) -> dict[str, bool]:
    return {"banner": target.custom_banner, "cover": target.custom_cover, "icon": target.custom_icon}


def _fsync_directory(path: Path) -> None:
    descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def _atomic_copy(source: Path, destination: Path) -> None:
    _ensure_owned_directory(destination.parent, create=True)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{destination.name}.", dir=destination.parent)
    try:
        with os.fdopen(descriptor, "wb") as output, source.open("rb") as input_file:
            shutil.copyfileobj(input_file, output)
            output.flush()
            os.fsync(output.fileno())
        os.chmod(temporary, 0o600)
        os.replace(temporary, destination)
        _fsync_directory(destination.parent)
    finally:
        Path(temporary).unlink(missing_ok=True)


def _write_manifest(path: Path, payload: dict[str, object], *, replace: bool = False) -> None:
    temporary = path.with_name(f".{path.name}.new")
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
    descriptor = os.open(temporary if replace else path, flags, 0o600)
    output_path = temporary if replace else path
    try:
        with os.fdopen(descriptor, "w") as output:
            json.dump(payload, output, indent=2, sort_keys=True)
            output.write("\n")
            output.flush()
            os.fsync(output.fileno())
        if replace:
            os.replace(output_path, path)
        _fsync_directory(path.parent)
    finally:
        if replace:
            temporary.unlink(missing_ok=True)


def _restore_originals(originals: list[dict[str, object]], journal: Path) -> None:
    for item in originals:
        destination = Path(str(item["path"]))
        if bool(item["present"]):
            _atomic_copy(journal / str(item["backup"]), destination)
        else:
            destination.unlink(missing_ok=True)


def apply_art(
    shortcuts: Path,
    shortcut: dict[str, object],
    target: LutrisTarget,
    backend: object,
    client: SteamGridDB,
    game_id: int,
    art_ids: dict[str, int],
    baseline: Path | None,
) -> list[Path]:
    appid = int(shortcut["appid"])
    existing = classify_existing(shortcuts, appid, target, baseline)
    if existing.protected or existing.lutris_custom_types:
        detail = [p.name for p in existing.protected] + list(existing.lutris_custom_types)
        print(f"SKIP {target.title!r}: existing custom art ({', '.join(detail)})")
        return []
    observed_defaults = {
        path: sha256(path)
        for path in existing.replaceable_defaults + tuple(path for path in target.media_candidates if path.exists())
    }
    journal = state_directory(target.game_id, appid)
    state_root = journal.parent
    _ensure_owned_directory(state_root, create=True)
    lock_path = state_root / ".lock"
    lock_descriptor = os.open(lock_path, os.O_RDWR | os.O_CREAT, 0o600)
    with os.fdopen(lock_descriptor, "r+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if journal.exists():
            raise ArtworkError(f"artwork journal already exists; review or roll it back first: {journal}")
        with tempfile.TemporaryDirectory(prefix=".artwork-stage-", dir=state_root) as temporary:
            stage = Path(temporary)
            selected: dict[str, dict[str, object]] = {}
            sources: dict[str, Path] = {}
            for kind in ART_REQUESTS:
                item = client.art(game_id, kind, art_ids[kind])
                selected[kind] = item
                output = stage / f"steam-{kind}.png"
                download_png(str(item["url"]), output, client.base)
                sources[kind] = output
            lutris_stage: dict[str, Path] = {}
            for kind, (source_kind, size) in LUTRIS_DERIVATIVES.items():
                output = stage / f"lutris-{kind}.png"
                derive_lutris(sources[source_kind], output, size)
                lutris_stage[kind] = output
            latest_target = resolve_lutris_target(target.game_id, backend)
            if latest_target.slug != target.slug or latest_target.destinations != target.destinations:
                raise ArtworkError("Lutris game identity or media paths changed while fetching")
            rechecked = classify_existing(shortcuts, appid, latest_target, baseline)
            current_defaults = {
                path: sha256(path)
                for path in rechecked.replaceable_defaults + tuple(path for path in latest_target.media_candidates if path.exists())
            }
            if (
                rechecked.protected
                or rechecked.lutris_custom_types
                or rechecked.replaceable_defaults != existing.replaceable_defaults
                or current_defaults != observed_defaults
            ):
                raise ArtworkError("custom/default art state changed while fetching; refusing to overwrite")
            journal.mkdir(mode=0o700)
            originals: list[dict[str, object]] = []
            old_flags = _old_flags(target)
            created: list[Path] = []
            mutation_started = False
            try:
                originals_dir = journal / "original"
                originals_dir.mkdir(mode=0o700)
                original_paths = sorted(set(existing.replaceable_defaults + tuple(path for path in target.media_candidates if path.exists())), key=str)
                applied_destinations = list(steam_destinations(shortcuts, appid).values()) + list(target.destinations.values())
                for destination in applied_destinations:
                    if destination.exists() and destination not in original_paths:
                        raise ArtworkError(f"unexpected destination appeared before commit: {destination}")
                for index, path in enumerate(sorted(set(original_paths + applied_destinations), key=str)):
                    if path.exists():
                        if path.is_symlink() or not path.is_file() or path.stat().st_uid != os.getuid():
                            raise ArtworkError(f"unsafe existing artwork path: {path}")
                        backup = f"original/{index}"
                        _atomic_copy(path, journal / backup)
                        originals.append({"path": str(path), "present": True, "backup": backup, "sha256": sha256(path)})
                    else:
                        originals.append({"path": str(path), "present": False, "backup": "", "sha256": ""})
                planned_sources = {
                    **{destination: sources[kind] for kind, destination in steam_destinations(shortcuts, appid).items()},
                    **{destination: lutris_stage[kind] for kind, destination in target.destinations.items()},
                }
                applied = [{"path": str(path), "sha256": sha256(source)} for path, source in planned_sources.items()]
                manifest = {
                    "version": 1,
                    "state": "prepared",
                    "lutris_id": target.game_id,
                    "slug": target.slug,
                    "steam_appid": appid,
                    "shortcuts": str(shortcuts.resolve()),
                    "sgdb_game_id": game_id,
                    "art_ids": art_ids,
                    "old_flags": old_flags,
                    "originals": originals,
                    "applied": applied,
                }
                _write_manifest(journal / "manifest.json", manifest)
                mutation_started = True
                for path in original_paths:
                    path.unlink(missing_ok=True)
                for destination, source in planned_sources.items():
                    _atomic_copy(source, destination)
                    created.append(destination)
                backend.set_custom_flags(target.game_id, {"banner": True, "cover": True, "icon": True})
                if any(sha256(path) != item["sha256"] for path, item in zip(created, applied, strict=True)):
                    raise ArtworkError("applied artwork digest verification failed")
                manifest["state"] = "committed"
                _write_manifest(journal / "manifest.json", manifest, replace=True)
                if baseline is not None:
                    baseline.unlink(missing_ok=True)
            except Exception as exc:
                rollback_error: Exception | None = None
                if mutation_started:
                    try:
                        _restore_originals(originals, journal)
                        backend.set_custom_flags(target.game_id, old_flags)
                    except Exception as restore_exc:  # Preserve recovery material on rollback failure.
                        rollback_error = restore_exc
                if rollback_error is None:
                    shutil.rmtree(journal, ignore_errors=True)
                    raise
                raise ArtworkError(
                    f"artwork apply failed and automatic restoration also failed; recovery journal retained at {journal}: {rollback_error}"
                ) from exc
    print(f"APPLIED {target.title!r} lutris_id={target.game_id} appid={appid} sgdb={game_id}: eight artwork objects")
    return created


def _load_manifest(path: Path) -> dict[str, object]:
    _ensure_owned_regular(path)
    if path.stat().st_mode & 0o077:
        raise ArtworkError(f"manifest must not grant group/other access: {path}")
    try:
        payload = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        raise ArtworkError(f"invalid artwork manifest: {path}") from exc
    if payload.get("version") != 1:
        raise ArtworkError("unsupported artwork manifest version")
    return payload


def _steam_candidate_paths(shortcuts: Path, appid: int) -> set[Path]:
    grid = shortcuts.parent / "grid"
    suffixes = ("", "p", "_hero", "_logo", "_icon")
    extensions = (".png", ".jpg", ".jpeg", ".webp", ".ico")
    return {grid / f"{appid}{suffix}{extension}" for suffix in suffixes for extension in extensions}


def _validate_manifest_paths(
    manifest: dict[str, object],
    journal: Path,
    shortcuts: Path,
    appid: int,
    target: LutrisTarget,
) -> tuple[list[dict[str, object]], list[dict[str, object]], dict[str, object]]:
    state = manifest.get("state")
    if state not in {"prepared", "committed", "rolling_back"}:
        raise ArtworkError("artwork manifest has an invalid transaction state")
    applied = manifest.get("applied")
    originals = manifest.get("originals")
    old_flags = manifest.get("old_flags")
    if not isinstance(applied, list) or not isinstance(originals, list) or not isinstance(old_flags, dict):
        raise ArtworkError("artwork manifest is incomplete")
    expected_applied = set(steam_destinations(shortcuts, appid).values()) | set(target.destinations.values())
    applied_paths = {Path(str(item.get("path"))) for item in applied if isinstance(item, dict)}
    if len(applied) != 8 or applied_paths != expected_applied:
        raise ArtworkError("artwork manifest applied paths do not match the exact game targets")
    if not all(isinstance(item, dict) and re.fullmatch(r"[0-9a-f]{64}", str(item.get("sha256", ""))) for item in applied):
        raise ArtworkError("artwork manifest has invalid applied digests")
    allowed_originals = _steam_candidate_paths(shortcuts, appid) | set(target.media_candidates)
    original_root = (journal / "original").resolve()
    for item in originals:
        if not isinstance(item, dict):
            raise ArtworkError("artwork manifest has an invalid original entry")
        destination = Path(str(item.get("path")))
        if destination not in allowed_originals:
            raise ArtworkError(f"artwork manifest original path escapes exact game targets: {destination}")
        present = item.get("present")
        if not isinstance(present, bool):
            raise ArtworkError("artwork manifest has an invalid original-presence marker")
        backup = str(item.get("backup", ""))
        original_digest = str(item.get("sha256", ""))
        if present:
            if not re.fullmatch(r"[0-9a-f]{64}", original_digest):
                raise ArtworkError("artwork manifest has an invalid original digest")
            if not re.fullmatch(r"original/\d+", backup):
                raise ArtworkError("artwork manifest has an unsafe backup path")
            backup_path = journal / backup
            if backup_path.resolve().parent != original_root:
                raise ArtworkError("artwork manifest backup path escapes the journal")
            _ensure_owned_regular(backup_path)
            if sha256(backup_path) != original_digest:
                raise ArtworkError("artwork manifest backup digest does not match")
        elif backup or original_digest:
            raise ArtworkError("artwork manifest has backup metadata for an absent original")
    if set(old_flags) != set(CUSTOM_FLAGS) or not all(isinstance(old_flags[kind], bool) for kind in CUSTOM_FLAGS):
        raise ArtworkError("artwork manifest has invalid old Lutris flags")
    return applied, originals, old_flags


def rollback_art(shortcuts: Path, shortcut: dict[str, object], target: LutrisTarget, backend: object, preview_only: bool) -> None:
    appid = int(shortcut["appid"])
    journal = state_directory(target.game_id, appid)
    manifest_path = journal / "manifest.json"
    manifest = _load_manifest(manifest_path)
    if manifest.get("lutris_id") != target.game_id or manifest.get("slug") != target.slug or manifest.get("steam_appid") != appid or manifest.get("shortcuts") != str(shortcuts.resolve()):
        raise ArtworkError("artwork manifest does not match the exact Lutris/Steam shortcut")
    applied, originals, old_flags = _validate_manifest_paths(manifest, journal, shortcuts, appid, target)
    transaction_state = str(manifest["state"])
    originals_by_path = {str(item["path"]): item for item in originals}

    def validate_current() -> None:
        for item in applied:
            path = Path(str(item["path"]))
            if not path.exists():
                if transaction_state == "committed":
                    raise ArtworkError(f"applied artwork is absent; refusing rollback: {path}")
                continue
            _ensure_owned_regular(path)
            digest = sha256(path)
            allowed = {str(item["sha256"])}
            original = originals_by_path.get(str(path))
            if transaction_state in {"prepared", "rolling_back"} and original is not None and bool(original["present"]):
                allowed.add(str(original["sha256"]))
            if digest not in allowed:
                raise ArtworkError(f"artwork changed after apply/preparation; refusing rollback: {path}")

    validate_current()
    print(f"ROLLBACK-PREVIEW lutris_id={target.game_id} appid={appid} journal={journal} state={transaction_state} files={len(applied)}")
    if preview_only:
        return
    state_root = journal.parent
    lock_descriptor = os.open(state_root / ".lock", os.O_RDWR | os.O_CREAT, 0o600)
    with os.fdopen(lock_descriptor, "r+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        validate_current()
        if transaction_state != "rolling_back":
            manifest["state"] = "rolling_back"
            _write_manifest(manifest_path, manifest, replace=True)
            transaction_state = "rolling_back"
        for item in applied:
            path = Path(str(item["path"]))
            if path.exists() and sha256(path) == item["sha256"]:
                path.unlink()
        try:
            _restore_originals(originals, journal)
            backend.set_custom_flags(target.game_id, {kind: bool(old_flags[kind]) for kind in CUSTOM_FLAGS})
        except Exception as exc:
            raise ArtworkError(f"rollback failed; recovery journal retained at {journal}: {exc}") from exc
        shutil.rmtree(journal)
        _fsync_directory(state_root)
    print(f"ROLLED-BACK lutris_id={target.game_id} appid={appid}")


def refresh_steam(appid: int) -> None:
    if subprocess.run(["pgrep", "-x", "steam"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode != 0:
        return
    steam = shutil.which("steam")
    if steam is None:
        print("NOTICE Steam is running but its command is unavailable; reopen Steam to refresh artwork", file=sys.stderr)
        return
    try:
        subprocess.run([steam, f"steam://nav/games/details/{appid}"], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=20)
        print(f"REFRESH requested Steam details reload for appid={appid}")
    except (subprocess.SubprocessError, OSError) as exc:
        print(f"NOTICE Steam refresh failed ({exc}); reopen Steam to load artwork", file=sys.stderr)


def _one_shortcut(path: Path, lutris_id: str) -> dict[str, object]:
    matches = lutris_shortcuts(path, lutris_id)
    if len(matches) != 1:
        raise ArtworkError(f"expected exactly one Steam shortcut for Lutris ID {lutris_id} in {path}; found {len(matches)}")
    return matches[0]


def preview(shortcuts: Path, shortcut: dict[str, object], target: LutrisTarget, baseline: Path | None) -> int:
    if normalize_title(str(shortcut["title"])) != normalize_title(target.title):
        raise ArtworkError("Lutris and Steam shortcut titles do not match")
    appid = int(shortcut["appid"])
    existing = classify_existing(shortcuts, appid, target, baseline)
    state = "skip-existing-custom-art" if existing.protected or existing.lutris_custom_types else "would-resolve-eight-artwork-destinations"
    steam_paths = ",".join(str(path) for path in steam_destinations(shortcuts, appid).values())
    lutris_paths = ",".join(str(path) for path in target.destinations.values())
    protected = ",".join(path.name for path in existing.protected) or "none"
    replaceable = ",".join(path.name for path in existing.replaceable_defaults) or "none"
    lutris_custom = ",".join(existing.lutris_custom_types) or "none"
    print(f"PREVIEW {target.title!r} lutris_id={target.game_id} slug={target.slug} appid={appid} shortcuts={shortcuts} grid={shortcuts.parent / 'grid'} steam={steam_paths} lutris={lutris_paths} journal={state_directory(target.game_id, appid) / 'manifest.json'} protected={protected} replaceable_defaults={replaceable} lutris_custom={lutris_custom} action={state} effect=close-reopen-lutris-and-refresh-steam rollback=digest-guarded")
    return 0


def self_check() -> int:
    try:
        import PIL
        pillow_version = PIL.__version__
    except ImportError as exc:
        raise ArtworkError("Pillow is unavailable") from exc
    backend = LutrisBackend()
    roots = backend.media_roots()
    shortcuts = find_shortcut_files(None)
    try:
        version_result = subprocess.run(["lutris", "--version"], check=True, capture_output=True, text=True, timeout=15)
        lutris_version = version_result.stdout.strip().splitlines()[-1]
    except (OSError, subprocess.SubprocessError, IndexError):
        lutris_version = "unknown"
    state_root = state_directory("self-check", 0).parent
    ancestor = state_root
    while not ancestor.exists() and ancestor != ancestor.parent:
        ancestor = ancestor.parent
    _ensure_owned_directory(ancestor)
    print(f"SELF-CHECK python={sys.version.split()[0]} pillow={pillow_version} lutris={lutris_version} lutris_api=available banner_root={roots[0]} cover_root={roots[1]} icon_root={roots[2]} state_root={state_root} state_ancestor={ancestor} steam_accounts={len(shortcuts)}")
    return 0


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--shortcuts", type=Path, help="Exact Steam shortcuts.vdf path")
    parser.add_argument("--lutris-id", help="Exact Lutris rungameid")
    parser.add_argument("--baseline", type=Path, help="Trusted pre-shortcut grid baseline")
    parser.add_argument("--capture-steam-baseline", action="store_true")
    parser.add_argument("--preview", action="store_true", help="Show exact targets without network or writes")
    parser.add_argument("--resolve-sgdb", action="store_true", help="Resolve exact SteamGridDB game/art IDs without image writes")
    parser.add_argument("--rollback", action="store_true", help="Rollback this helper's journaled art set")
    parser.add_argument("--self-check", action="store_true")
    parser.add_argument("--sgdb-game-id", type=int)
    for kind, argument in ART_ID_ARGUMENTS.items():
        parser.add_argument(f"--{argument.replace('_', '-')}", dest=argument, type=int)
    parser.add_argument("--api-key-file", type=Path, default=Path("~/.config/steam-shortcut-artwork/steamgriddb-api-key").expanduser())
    parser.add_argument("--api-base", default=API_BASE, help=argparse.SUPPRESS)
    parser.add_argument("--no-refresh", action="store_true")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None, backend: object | None = None) -> int:
    args = parse_args(argv)
    if args.self_check:
        return self_check()
    if args.capture_steam_baseline:
        if args.shortcuts is None or args.baseline is None:
            raise ArtworkError("baseline capture requires --shortcuts and --baseline")
        capture_baseline(args.shortcuts.expanduser().resolve(), args.baseline.expanduser().resolve())
        return 0
    if args.shortcuts is None:
        discovered = find_shortcut_files(None)
        detail = ", ".join(str(path) for path in discovered) or "none"
        raise ArtworkError(f"pass one exact --shortcuts path; discovered: {detail}")
    if args.lutris_id is None:
        raise ArtworkError("--lutris-id is required")
    args.shortcuts = args.shortcuts.expanduser().resolve()
    if args.baseline is not None:
        args.baseline = args.baseline.expanduser().resolve()
    _ensure_owned_regular(args.shortcuts)
    selected_backend = backend or LutrisBackend()
    target = resolve_lutris_target(args.lutris_id, selected_backend)
    shortcut = _one_shortcut(args.shortcuts, args.lutris_id)
    if args.rollback:
        rollback_art(args.shortcuts, shortcut, target, selected_backend, args.preview)
        return 0
    if args.preview:
        return preview(args.shortcuts, shortcut, target, args.baseline)
    if normalize_title(str(shortcut["title"])) != normalize_title(target.title):
        raise ArtworkError("Lutris and Steam shortcut titles do not match")
    existing = classify_existing(args.shortcuts, int(shortcut["appid"]), target, args.baseline)
    if existing.protected or existing.lutris_custom_types:
        detail = [p.name for p in existing.protected] + list(existing.lutris_custom_types)
        print(f"SKIP {target.title!r}: existing custom art ({', '.join(detail)})")
        return 0
    key = read_api_key(args.api_key_file)
    client = SteamGridDB(key, args.api_base)
    sgdb_id, sgdb_title = client.game(target.title, args.sgdb_game_id)
    if args.resolve_sgdb:
        resolved = {kind: client.art(sgdb_id, kind) for kind in ART_REQUESTS}
        detail = " ".join(
            f"{kind}_id={item['id']} dimensions={item.get('width')}x{item.get('height')} style={item.get('style', '')} host={urllib.parse.urlparse(str(item.get('url', ''))).hostname}"
            for kind, item in resolved.items()
        )
        print(f"RESOLVED sgdb_game_id={sgdb_id} sgdb_title={sgdb_title!r} {detail}")
        return 0
    if args.sgdb_game_id is None:
        raise ArtworkError("apply requires the approved --sgdb-game-id")
    art_ids: dict[str, int] = {}
    for kind, argument in ART_ID_ARGUMENTS.items():
        value = getattr(args, argument)
        if value is None:
            raise ArtworkError(f"apply requires --{argument.replace('_', '-')}")
        art_ids[kind] = value
    created = apply_art(args.shortcuts, shortcut, target, selected_backend, client, sgdb_id, art_ids, args.baseline)
    if created and not args.no_refresh:
        refresh_steam(int(shortcut["appid"]))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ArtworkError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
