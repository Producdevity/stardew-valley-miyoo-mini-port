#!/bin/sh
set -eu
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)

case ${1:-} in
    --allow-untested-build)
        export SVMM_ALLOW_UNTESTED_GAMEFILES=1
        shift
        ;;
    --*)
        echo "usage: $0 [--allow-untested-build] [gamefiles directory] [output directory]" >&2
        exit 2
        ;;
esac

if [ "$#" -gt 2 ]; then
    echo "usage: $0 [--allow-untested-build] [gamefiles directory] [output directory]" >&2
    exit 2
fi

GAMEFILES=${1:-$ROOT/gamefiles}
OUTPUT=${2:-$ROOT/OnionOS-package}
"$ROOT/scripts/prepare-game.sh" "$GAMEFILES" "$OUTPUT"
