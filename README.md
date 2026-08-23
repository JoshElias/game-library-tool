# game-library

A host-targeted operator for Lutris installs, SteamGridDB art, and
Ludusavi save lineage. It is a CLI, not a desktop GUI and not Ansible.

[![ci](https://github.com/JoshElias/game-library-tool/actions/workflows/ci.yml/badge.svg)](https://github.com/JoshElias/game-library-tool/actions/workflows/ci.yml)

This public tree does **not** include anyone's store library, SSH keys,
host inventory, or SteamGridDB pins. Bring your own hosts file and
recipes.

## Install

```bash
git clone https://github.com/JoshElias/game-library-tool.git
cd game-library-tool
cargo install --path . --locked --force
```

## Configure

Copy the examples and edit them:

```bash
mkdir -p ~/.config/game-library
cp examples/config.yaml ~/.config/game-library/config.yaml
cp examples/hosts.yaml ~/.config/game-library/hosts.yaml
```

Point the operator at the hosts file:

```bash
export GAME_LIBRARY_HOSTS=$HOME/.config/game-library/hosts.yaml
# optional
export GAME_LIBRARY_CONFIG=$HOME/.config/game-library/config.yaml
export GAME_LIBRARY_REGISTRY=$PWD/registry/games
```

Without a hosts file, `hosts` / `doctor` operate on the current machine
only (`ssh` alias `local`). A file at
`~/.config/game-library/hosts.yaml` is picked up automatically.

Install the lineage helper on each endpoint desktop user:

```bash
install -m 755 vendor/ludusavi-lutris-wrap ~/.local/bin/ludusavi-lutris-wrap
```

Configure your own rclone remote name with `GAME_LIBRARY_LINEAGE_REMOTE`
or `lineage_remote` in the config file. Do not commit credentials.

## Commands

```bash
game-library hosts
game-library doctor
game-library list --host example
game-library registry list
game-library status <slug> --host <name>
game-library install <slug> --host <name>
game-library verify <slug> --host <name>
game-library uninstall <slug> --host <name>
```

`install` and `uninstall` print a plan and wait for `go`.

## What stays out of this repo

- Production Ansible inventory, IPs, and SSH aliases
- Personal GOG/Steam recipes and SteamGridDB art IDs
- rclone / Nextcloud / SteamGridDB key files
- Endpoint Wine prefixes and save archives

`registry/games/example-game.yaml` is a non-installable placeholder.

## License

MIT
