#!/usr/bin/env python3
"""Print the unique committed lineage root. No credentials, no save contents."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys

REMOTE_ROOT = "Games/Game Saves/.lineage/v1"


def game_key(name: str) -> str:
    return hashlib.sha256(name.encode("utf-8")).hexdigest()


def rclone_lsf(rclone: str, spec: str) -> list[str]:
    result = subprocess.run(
        [rclone, "lsf", spec, "--dirs-only"],
        check=False,
        text=True,
        capture_output=True,
    )
    if result.returncode != 0:
        raise SystemExit(f"harvest-failed:{result.returncode}")
    return [line.strip().strip("/") for line in result.stdout.splitlines() if line.strip()]


def rclone_cat(rclone: str, spec: str) -> dict:
    result = subprocess.run(
        [rclone, "cat", spec],
        check=False,
        text=True,
        capture_output=True,
    )
    if result.returncode != 0:
        raise SystemExit(f"harvest-failed:{result.returncode}")
    return json.loads(result.stdout)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--name", required=True)
    parser.add_argument("--rclone", default="/usr/bin/rclone")
    parser.add_argument("--remote", default="ludusavi")
    args = parser.parse_args()
    base = f"{args.remote}:{REMOTE_ROOT}/games/{game_key(args.name)}/generations"
    roots = []
    for ident in rclone_lsf(args.rclone, f"{base}/"):
        manifest = rclone_cat(args.rclone, f"{base}/{ident}/manifest.json")
        generation = manifest.get("generation") if isinstance(manifest, dict) else None
        if not isinstance(generation, dict):
            continue
        if generation.get("parent") is None and generation.get("id") == ident:
            roots.append(ident)
    if not roots:
        print("none")
        return 1
    if len(roots) != 1:
        print(f"competing {len(roots)}")
        return 2
    print(f"unique {roots[0]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
