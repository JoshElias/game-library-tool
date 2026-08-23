#!/usr/bin/env python3
"""Set Steam launch options to wrap a game. Fails if Steam is running."""

from __future__ import annotations

import argparse
import shutil
import subprocess
from pathlib import Path


def steam_running() -> bool:
    for pattern in ("steamwebhelper",):
        if subprocess.run(["pgrep", "-f", pattern], stdout=subprocess.DEVNULL).returncode == 0:
            return True
    return subprocess.run(["pgrep", "-x", "steam"], stdout=subprocess.DEVNULL).returncode == 0


def userdata_configs(home: Path) -> list[Path]:
    root = home / ".local/share/Steam/userdata"
    if not root.is_dir():
        return []
    return sorted(root.glob("*/config/localconfig.vdf"))


def wrap_command(user: str, name: str) -> str:
    escaped = name.replace('"', '\\"')
    return (
        f"/home/{user}/.local/bin/ludusavi-lutris-wrap run "
        f'--name "{escaped}" --ludusavi /usr/bin/ludusavi '
        f"--rclone /usr/bin/rclone -- %command%"
    )


def apply(path: Path, appid: str, command: str) -> str:
    text = path.read_text()
    marker = f'"{appid}"\n\t\t\t\t\t{{\n'
    idx = text.find(marker)
    if idx < 0:
        raise SystemExit(f"no apps block for {appid}")
    start = idx + len(marker)
    end = text.find("\n\t\t\t\t\t}", start)
    if end < 0:
        raise SystemExit("apps block is malformed")
    block = text[start:end]
    indent = "\t\t\t\t\t\t"
    line = f'{indent}"LaunchOptions"\t\t"{command}"\n'
    if '"LaunchOptions"' in block:
        if command in block and "ludusavi-lutris-wrap" in block:
            return "already"
        lines = []
        for raw in block.splitlines(keepends=True):
            if '"LaunchOptions"' in raw:
                lines.append(line)
            else:
                lines.append(raw)
        block = "".join(lines)
        action = "replaced"
    else:
        block = line + block
        action = "added"
    backup = path.with_name(path.name + ".pre-steam-wrap")
    if not backup.exists():
        shutil.copy2(path, backup)
        backup.chmod(0o600)
    path.write_text(text[:start] + block + text[end:])
    return action


def wrapped(path: Path, appid: str) -> bool:
    text = path.read_text(errors="replace")
    marker = f'"{appid}"'
    idx = 0
    while True:
        hit = text.find(marker, idx)
        if hit < 0:
            return False
        window = text[hit : hit + 800]
        if "ludusavi-lutris-wrap" in window and "%command%" in window:
            return True
        idx = hit + len(marker)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--appid", required=True)
    parser.add_argument("--user", required=True)
    parser.add_argument("--name", required=True)
    parser.add_argument("--home", type=Path, default=Path.home())
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    configs = userdata_configs(args.home)
    if args.check:
        print("wrapped" if any(wrapped(path, args.appid) for path in configs) else "missing")
        return 0
    if steam_running():
        raise SystemExit("Steam is running; close it before applying launch options")
    if not configs:
        raise SystemExit("no Steam localconfig.vdf")
    command = wrap_command(args.user, args.name)
    actions = [apply(path, args.appid, command) for path in configs]
    print(actions[0] if len(set(actions)) == 1 else ",".join(actions))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
