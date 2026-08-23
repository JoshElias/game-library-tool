# game-library

Keep **your** games and **your** saves in order across the PCs and
handhelds you already own.

[![ci](https://github.com/JoshElias/game-library-tool/actions/workflows/ci.yml/badge.svg)](https://github.com/JoshElias/game-library-tool/actions/workflows/ci.yml)

You point this CLI at machines you control. It installs an owned GOG
title through Lutris (or enrolls a game that is already on disk), pins
artwork, and wraps launch so a quit publishes an append-only save
generation. The next machine restores that generation before play.
Nothing here is Steam Cloud, Dropbox, or a shared account.

This repo does **not** ship your library, SSH keys, or store pins.
Those stay on your machines.

## What you get

- One recipe per game: store ID, install path, art IDs, Ludusavi name
- One hosts file for every device: desktop user, SSH, XDG game roots
- `install` on a host: download if needed, wrap, enroll saves
- `list` / `status`: what is actually installed and enrolled
- `uninstall`: trash that game directory; keep the store record and
  save lineage
- After a normal quit, the wrap helper publishes a new generation.
  The other enrolled host restores it on the next launch.

Saves sync only under your private remote, beneath
`Games/Game Saves/.lineage/`. Conflicts stay as branches. Nothing
auto-picks a winner.

## Set up once

1. Install the CLI and the wrap helper on each device you play on.
2. Copy `examples/hosts.yaml` to `~/.config/game-library/hosts.yaml`
   and name your machines.
3. Put your rclone remote name in
   `~/.config/game-library/config.yaml` (see `examples/config.yaml`).
   Do not commit that file if it points at real credentials.
4. Add a recipe under `registry/games/` for each title you own.
   Harvest IDs from your GOG/Lutris library and SteamGridDB; do not
   invent them. `example-game.yaml` is a placeholder.

```bash
git clone https://github.com/JoshElias/game-library-tool.git
cd game-library-tool
cargo install --path . --locked --force
install -m 755 vendor/ludusavi-lutris-wrap ~/.local/bin/ludusavi-lutris-wrap
mkdir -p ~/.config/game-library
cp examples/hosts.yaml ~/.config/game-library/hosts.yaml
cp examples/config.yaml ~/.config/game-library/config.yaml
```

`~/.config/game-library/hosts.yaml` is found automatically.

## Day to day

```bash
game-library hosts
game-library doctor --host living-room
game-library list --host living-room
game-library install hades --host living-room    # waits for go
game-library install hades --host steam-deck     # import the same lineage
game-library status hades --host steam-deck
game-library verify hades --host steam-deck
game-library uninstall hades --host living-room  # waits for go; keeps saves
```

Play from the Lutris or Steam tile the wrap already owns. Close the
game normally so a generation can publish. A dead Lutris probe is
usually SSH or no graphical session, not a missing install.

## Commands

| Command | What it does |
| --- | --- |
| `hosts` | Machines in your hosts file (or this machine only) |
| `doctor` | Session, Lutris, Ludusavi, wrap helper, paths |
| `list` | Live installs vs recipes, including unregistered Lutris titles |
| `registry` | Add/show/remove pinned recipes |
| `status` / `verify` | One game on one host |
| `install` | Plan, then `go`: install, art, enroll, wrap |
| `uninstall` | Plan, then `go`: trash files only |

## Configuration

| Need | Env | Then | Default |
| --- | --- | --- | --- |
| Hosts file | `GAME_LIBRARY_HOSTS` | `hosts:` in config | `~/.config/game-library/hosts.yaml` |
| Config | `GAME_LIBRARY_CONFIG` | | `~/.config/game-library/config.yaml` |
| Recipes | `GAME_LIBRARY_REGISTRY` | `$GAME_LIBRARY_REPO/registry/games` | `./registry/games` or XDG |
| SSH | `GAME_LIBRARY_SSH` | `ssh_helper` in config | `ssh` |
| Lineage remote | `GAME_LIBRARY_LINEAGE_REMOTE` | `lineage_remote` in config | `ludusavi` |
| Shared GOG root | `GAME_LIBRARY_SHARED_GOG` | `shared_gog_root` in config | unset |

## Develop

```bash
cargo test --locked
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo run -- hosts
```

Rust 1.88 is pinned in `rust-toolchain.toml`. The CLI orchestrates.
Endpoint Python (`scripts/`, `vendor/ludusavi-lutris-wrap`) talks to
Lutris, Pillow, Ludusavi, and rclone.

## License

MIT
