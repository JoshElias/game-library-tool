#!/usr/bin/env python3
"""Write a Lutris prefix_command so colons in the game name survive YAML."""

from __future__ import annotations

import argparse
import os
import shutil
import sys
from pathlib import Path

from lutris import settings
from lutris.database import games

try:
    import yaml
except ImportError:
    yaml = None


class _Dumper(yaml.SafeDumper if yaml is not None else object):
    pass


def _represent_str(dumper, data: str):
    style = '"' if any(ch in data for ch in ":#{}[]&*!?|>%@`'\"") else None
    return dumper.represent_scalar("tag:yaml.org,2002:str", data, style=style)


if yaml is not None:
    _Dumper.add_representer(str, _represent_str)


def _config_path(lutris_id: str) -> Path:
    record = games.get_game_by_field(str(lutris_id), "id")
    if not record or not record.get("configpath"):
        raise SystemExit(f"no Lutris config for {lutris_id}")
    return Path(settings.CONFIG_DIR) / "games" / f"{record['configpath']}.yml"


def _load(path: Path) -> dict:
    if yaml is None:
        raise SystemExit("pyyaml missing")
    data = yaml.safe_load(path.read_text()) or {}
    if not isinstance(data, dict):
        raise SystemExit("lutris config is not a mapping")
    return data


def _dump(data: dict) -> str:
    return yaml.dump(
        data,
        Dumper=_Dumper,
        sort_keys=False,
        allow_unicode=True,
        width=4096,
    )


def classify(prefix: str) -> str:
    if "ludusavi-lutris-wrap" in prefix and "--name" in prefix and prefix.rstrip().endswith("--"):
        return "ours"
    if prefix.strip():
        return "other"
    return "none"


def prefix_intact(prefix: str, name: str) -> bool:
    return name in prefix and prefix.rstrip().endswith("--") and "ludusavi-lutris-wrap" in prefix


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--lutris-id", required=True)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--prefix")
    parser.add_argument("--prefix-file", type=Path)
    parser.add_argument("--backup", type=Path)
    args = parser.parse_args()
    path = _config_path(args.lutris_id)
    data = _load(path)
    system = data.get("system") if isinstance(data.get("system"), dict) else {}
    current = str(system.get("prefix_command") or "")
    kind = classify(current)
    if args.check:
        print(kind)
        return 0 if kind == "ours" else 1
    prefix = args.prefix
    if args.prefix_file:
        prefix = args.prefix_file.read_text().rstrip("\n")
    if not prefix or not args.backup:
        raise SystemExit("--prefix/--prefix-file and --backup are required to write")
    if kind == "other" and "ludusavi-lutris-wrap" not in current:
        print("other")
        return 2
    args.backup.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    shutil.copy2(path, args.backup)
    os.chmod(args.backup, 0o600)
    system = dict(system)
    system["prefix_command"] = prefix
    data["system"] = system
    dumped = _dump(data)
    first = dumped.split("prefix_command:", 1)[-1].splitlines()[0]
    if ":" in prefix and ":" not in first:
        raise SystemExit("prefix was folded across YAML lines")
    path.write_text(dumped)
    verify = str((_load(path).get("system") or {}).get("prefix_command") or "")
    if verify != prefix:
        raise SystemExit("prefix did not survive YAML round-trip")
    print("applied")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
