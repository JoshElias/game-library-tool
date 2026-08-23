# game-library

A host-targeted operator for Lutris installs, SteamGridDB art, and
Ludusavi save lineage. It is a CLI, not a desktop GUI and not Ansible.

[![ci](https://github.com/JoshElias/game-library-tool/actions/workflows/ci.yml/badge.svg)](https://github.com/JoshElias/game-library-tool/actions/workflows/ci.yml)

This tree does **not** ship anyone's store library, SSH keys, host
inventory, or SteamGridDB pins. Bring your own hosts file and recipes.

## 30-second loop

```bash
git clone https://github.com/JoshElias/game-library-tool.git
cd game-library-tool
cargo test --locked
cargo run -- hosts
cargo run -- doctor
```

Rust 1.88 is pinned in `rust-toolchain.toml`. CI runs `fmt`, `clippy
-D warnings`, and `cargo test --locked`.

## Install

```bash
cargo install --path . --locked --force
```

On each endpoint desktop user, install the lineage helper:

```bash
install -m 755 vendor/ludusavi-lutris-wrap ~/.local/bin/ludusavi-lutris-wrap
```

Do not commit rclone, Nextcloud, or SteamGridDB credentials.

## Configure

Copy the examples, then edit them:

```bash
mkdir -p ~/.config/game-library
cp examples/config.yaml ~/.config/game-library/config.yaml
cp examples/hosts.yaml ~/.config/game-library/hosts.yaml
```

`~/.config/game-library/hosts.yaml` is found automatically. You do not
need `GAME_LIBRARY_HOSTS` unless the file lives somewhere else.

Without a hosts file, `hosts` / `doctor` target this machine only
(`ssh` alias `local`).

| Need | Env | Then | Default |
| --- | --- | --- | --- |
| Hosts file | `GAME_LIBRARY_HOSTS` | `hosts:` in config | `~/.config/game-library/hosts.yaml` |
| Config | `GAME_LIBRARY_CONFIG` | | `~/.config/game-library/config.yaml` |
| Recipes | `GAME_LIBRARY_REGISTRY` | `$GAME_LIBRARY_REPO/registry/games` | `./registry/games` or XDG |
| SSH | `GAME_LIBRARY_SSH` | `ssh_helper` in config | `ssh` |
| Lineage remote | `GAME_LIBRARY_LINEAGE_REMOTE` | `lineage_remote` in config | `ludusavi` |
| Shared GOG root | `GAME_LIBRARY_SHARED_GOG` | `shared_gog_root` in config | unset (no redirect check) |

`registry/games/example-game.yaml` is a non-installable placeholder.

## Commands

```bash
game-library hosts
game-library doctor                 # or: --host <name>
game-library list --host <name>
game-library registry list
game-library status <slug> --host <name>
game-library install <slug> --host <name>
game-library verify <slug> --host <name>
game-library uninstall <slug> --host <name>
```

`install` and `uninstall` print a plan and wait for `go`. An empty
Lutris/wrap probe is usually SSH or a missing graphical session, not a
missing game.

## Hack on it

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo run -- list --host "$(hostname)"
```

Layout:

| Path | Role |
| --- | --- |
| `src/` | Rust CLI (clap). Orchestrates; does not speak Gtk. |
| `scripts/` | Endpoint Python. Lutris DB/Steam helpers and artwork. |
| `vendor/ludusavi-lutris-wrap` | Lineage helper installed on each desktop user. |
| `registry/games/` | Pinned recipes (yours, not this repo's). |
| `examples/` | Sample `hosts.yaml` and `config.yaml`. |
| `.github/workflows/ci.yml` | fmt + clippy + test. |

Rust decides what to run and when. Python stays because Lutris's
mutation API is Python, artwork uses Pillow, and the wrap helper is the
proven lineage protocol.

Pin a recipe before `install`. Harvest store IDs and SteamGridDB art IDs
from a live library; do not invent them.

## What stays out of this repo

- Production inventories, IPs, and SSH aliases
- Personal GOG/Steam recipes and SteamGridDB art IDs
- rclone / Nextcloud / SteamGridDB key files
- Endpoint Wine prefixes and save archives

## License

MIT
