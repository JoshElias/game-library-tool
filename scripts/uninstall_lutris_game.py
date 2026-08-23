#!/usr/bin/env python3
"""Trash one Lutris game directory and mark the entry uninstalled.

Keeps the GOG/service library row. Never deletes lineage or empties Trash.
Does not import lutris.game (Gtk).
"""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path
from types import SimpleNamespace

from lutris.database import games


def _record(lutris_id: str) -> dict:
    record = games.get_game_by_field(str(lutris_id), "id")
    if not record:
        raise SystemExit(f"no Lutris record {lutris_id}")
    return record


def _trash(path: Path) -> None:
    completed = subprocess.run(["gio", "trash", "--", str(path)], check=False)
    if completed.returncode != 0:
        raise SystemExit(f"gio trash failed for {path}")
    if path.exists():
        raise SystemExit(f"path still exists after gio trash: {path}")


def _restore(path: Path) -> None:
    listed = subprocess.run(
        ["gio", "trash", "--list"], check=False, capture_output=True, text=True
    )
    if listed.returncode != 0:
        raise SystemExit("gio trash --list failed during rollback")
    target = str(path)
    for line in listed.stdout.splitlines():
        parts = line.split("\t")
        if len(parts) >= 2 and parts[1].rstrip("/") == target:
            restore = subprocess.run(
                ["gio", "trash", "--restore", parts[0]], check=False
            )
            if restore.returncode != 0:
                raise SystemExit(f"gio trash --restore failed for {parts[0]}")
            return
    raise SystemExit(f"could not find {path} in Trash to restore")


def _mark_uninstalled(record: dict) -> None:
    game_id = games.update_existing(
        id=record["id"],
        name=record.get("name") or "",
        slug=record.get("slug") or "",
        runner="",
        directory="",
        installed=0,
    )
    if not game_id:
        raise SystemExit(f"lutris update_existing did not match {record['id']}")


def _remove_shortcut(record: dict) -> str:
    try:
        from lutris.util.steam.shortcut import remove_shortcut
    except Exception:
        return "shortcut-skip"
    game = SimpleNamespace(
        id=str(record["id"]),
        name=record.get("name") or "",
        slug=record.get("slug") or "",
        runner_name=record.get("runner") or "",
    )
    remove_shortcut(game)
    return "shortcut-removed"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--lutris-id", required=True)
    parser.add_argument("--directory", required=True)
    parser.add_argument("--slug", required=True)
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()
    record = _record(args.lutris_id)
    directory = Path(args.directory)
    payload = {
        "id": record.get("id"),
        "slug": record.get("slug"),
        "name": record.get("name"),
        "directory": record.get("directory"),
        "installed": record.get("installed"),
    }
    if str(record.get("slug") or "") != args.slug:
        raise SystemExit(
            f"slug mismatch: lutris={record.get('slug')} recipe={args.slug}"
        )
    if str(record.get("directory") or "").rstrip("/") != str(directory).rstrip("/"):
        raise SystemExit(
            f"directory mismatch: lutris={record.get('directory')} expected={directory}"
        )
    if directory.is_symlink():
        raise SystemExit(f"refusing to trash symlink {directory}")
    if not args.apply:
        print(json.dumps(payload, sort_keys=True))
        return 0
    if not directory.is_dir():
        raise SystemExit(f"missing install directory {directory}")
    _trash(directory)
    try:
        _mark_uninstalled(record)
        shortcut = _remove_shortcut(record)
    except Exception:
        if not directory.exists():
            _restore(directory)
        raise
    print(f"uninstalled id={record.get('id')} slug={args.slug} {shortcut}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
