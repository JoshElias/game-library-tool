#!/usr/bin/env python3
"""Export an inventory-free web build context; never use the private repo root."""
import argparse
import gzip
from pathlib import Path
import tarfile

FILES = ('Cargo.toml', 'Cargo.lock', 'crates/game-library/Cargo.toml',
         'crates/game-library/scripts/steam_inventory.py',
         'crates/game-library-web/Cargo.toml',
         'crates/game-library-web/Dockerfile',
         'crates/game-library-web/Dockerfile.external-inputs',
         'crates/game-library-web/container-entrypoint.sh')
DIRECTORIES = ('crates/game-library/src', 'crates/game-library-web/src',
               'crates/game-library-web/templates', 'crates/game-library-web/static',
               'crates/game-library-web/migrations')


def members(root):
    paths = [root / name for name in FILES]
    for name in DIRECTORIES:
        directory = root / name
        if directory.is_symlink() or not directory.is_dir():
            raise ValueError(f'required regular source directory missing: {name}')
        paths.extend(directory.rglob('*'))
    result = []
    for path in paths:
        if path.is_symlink() or any((root / part).is_symlink() for part in path.relative_to(root).parents):
            raise ValueError('symlinks are forbidden in the exported source context')
        if path.is_dir():
            continue
        if not path.is_file():
            raise ValueError('required regular source file missing')
        result.append(path)
    return sorted(result)


def export(root, output):
    source = members(root)  # Validate every source before creating the archive.
    with output.open('xb') as raw, gzip.GzipFile(fileobj=raw, mode='wb', mtime=0, filename='') as gz:
        with tarfile.open(fileobj=gz, mode='w') as archive:
            for path in source:
                info = archive.gettarinfo(str(path), str(path.relative_to(root)))
                info.uid = info.gid = info.mtime = 0
                info.uname = info.gname = ''
                with path.open('rb') as stream:
                    archive.addfile(info, stream)
    return len(source)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--output', type=Path, required=True, help='new archive, never overwritten')
    args = parser.parse_args()
    print(f'exported {export(args.source.resolve(), args.output)} source files')
