#!/usr/bin/env python3
"""Create or report one Lutris Steam shortcut without opening the Game GUI."""

from __future__ import annotations

import argparse
from types import SimpleNamespace

from lutris.database import games
from lutris.util.steam.shortcut import create_shortcut, shortcut_exists


def game_for(lutris_id: str) -> SimpleNamespace:
    record = games.get_game_by_field(str(lutris_id), "id")
    if not record:
        raise SystemExit(f"no Lutris record {lutris_id}")
    return SimpleNamespace(
        id=str(record["id"]),
        name=record["name"],
        slug=record["slug"],
        runner_name=record.get("runner") or "",
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--lutris-id", required=True)
    parser.add_argument("--exists", action="store_true")
    args = parser.parse_args()
    game = game_for(args.lutris_id)
    exists = shortcut_exists(game)
    if args.exists:
        print("exists" if exists else "missing")
        return 0 if exists else 1
    if exists:
        print(f"SHORTCUT-EXISTS lutris_id={game.id} name={game.name}")
        return 0
    create_shortcut(game)
    print(f"SHORTCUT-CREATED lutris_id={game.id} name={game.name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
