#!/bin/sh
# Explicit, external deployment contract. Never echo private paths or contents.
set -eu
if [ -z "${GAME_LIBRARY_HOSTS:-}" ] || [ ! -f "$GAME_LIBRARY_HOSTS" ] || [ ! -r "$GAME_LIBRARY_HOSTS" ]; then
    printf '%s\n' 'external hosts file required' >&2
    exit 2
fi
if [ -z "${GAME_LIBRARY_REGISTRY:-}" ] || [ ! -d "$GAME_LIBRARY_REGISTRY" ] || [ ! -r "$GAME_LIBRARY_REGISTRY" ]; then
    printf '%s\n' 'external registry directory required' >&2
    exit 2
fi
if [ -z "${GAME_LIBRARY_DATABASE_URL_FILE:-}" ] || [ ! -f "$GAME_LIBRARY_DATABASE_URL_FILE" ] || [ ! -r "$GAME_LIBRARY_DATABASE_URL_FILE" ]; then
    printf '%s\n' 'external database URL file required' >&2
    exit 2
fi
# Source-only validation of the contract, with no database connection or service.
if [ "${1:-}" = '--validate-inputs' ]; then
    exit 0
fi
exec /usr/local/bin/game-library-web "$@"
