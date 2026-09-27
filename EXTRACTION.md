# Public Game Library extract (v5)

Portable product candidate. Household inventory stays in daemon-fleet.

Included:
- Existing public CLI baseline (examples only; `example-game.yaml` only)
- Website image contract (`website/Dockerfile`, entrypoint, exporter)

Excluded:
- Private `hosts.yaml` and real recipe YAML
- Ansible inventory/roles, SSH routing, workstation docs
- Private crate modules whose tests/fixtures still embed household paths

Do not overwrite JoshElias/game-library-tool with the private crate. Public
API differences remain; this tree is the privacy-safe split, not a merge.

Website hosts/recipes/database URL are runtime mounts, never image layers.
