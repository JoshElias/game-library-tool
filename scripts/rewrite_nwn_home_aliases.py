#!/usr/bin/env python3
"""Rewrite Neverwinter Nights HD0 aliases to this host's home."""

from __future__ import annotations

import argparse
import re
from pathlib import Path

FILES = ("nwn.ini", "nwnplayer.ini", "settings.tml")
PATTERN = re.compile(r"/home/[^/]+/\.local/share/Neverwinter Nights")


def rewrite(home: Path) -> int:
    home = home.resolve()
    target = f"{home}/.local/share/Neverwinter Nights"
    root = home / ".local/share" / "Neverwinter Nights"
    changed = 0
    if not root.is_dir():
        return 0
    for name in FILES:
        path = root / name
        if not path.is_file():
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeError:
            continue
        updated = PATTERN.sub(target, text)
        if updated == text:
            continue
        backup = path.with_name(path.name + ".pre-home-rewrite")
        if not backup.exists():
            backup.write_text(text)
        path.write_text(updated)
        changed += 1
    return changed


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--home", type=Path, default=Path.home())
    args = parser.parse_args()
    print(f"rewritten {rewrite(args.home)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
