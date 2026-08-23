#!/usr/bin/env python3
"""Find an installed Steam app. Prints one JSON object. No credentials."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


def steam_roots(home: Path) -> list[Path]:
    roots: list[Path] = []
    default = home / ".local/share/Steam"
    extra = home / ".local/share/games/steam"
    folders = default / "steamapps" / "libraryfolders.vdf"
    if folders.is_file():
        for match in re.finditer(r'"path"\s+"([^"]+)"', folders.read_text(errors="replace")):
            roots.append(Path(match.group(1)))
    for path in (default, extra):
        if path.is_dir() and path not in roots:
            roots.append(path)
    return roots


def parse_acf(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in path.read_text(errors="replace").splitlines():
        match = re.search(r'"([^"]+)"\s+"([^"]*)"', line)
        if match:
            values[match.group(1)] = match.group(2)
    return values


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--appid", required=True)
    parser.add_argument("--home", type=Path, default=Path.home())
    args = parser.parse_args()
    for root in steam_roots(args.home):
        acf = root / "steamapps" / f"appmanifest_{args.appid}.acf"
        if not acf.is_file():
            continue
        data = parse_acf(acf)
        installdir = data.get("installdir") or ""
        directory = str(root / "steamapps" / "common" / installdir) if installdir else ""
        flags = int(data.get("StateFlags") or "0")
        installed = bool(flags & 4) and bool(directory) and Path(directory).is_dir()
        print(
            json.dumps(
                {
                    "installed": installed,
                    "directory": directory,
                    "name": data.get("name") or "",
                }
            )
        )
        return 0
    print(json.dumps({"installed": False, "directory": "", "name": ""}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
