#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: $0 <Stardew gamefiles directory>" >&2
    exit 2
fi

ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
SOURCE=$1
. "$ROOT/scripts/common.sh"

EXPECTED_XNB_COUNT=3550
ALLOW_UNTESTED=${SVMM_ALLOW_UNTESTED_GAMEFILES:-0}

case "$ALLOW_UNTESTED" in
    0|1) ;;
    *) fail 'SVMM_ALLOW_UNTESTED_GAMEFILES must be 0 or 1' ;;
esac

[ -d "$SOURCE" ] || fail "gamefiles directory does not exist: $SOURCE"
SOURCE=$(CDPATH='' cd -- "$SOURCE" && pwd)

for required in "Stardew Valley.exe" xTile.dll Content; do
    [ -e "$SOURCE/$required" ] || fail "missing game file: $required"
done

symlink=$(find "$SOURCE" -type l -print -quit)
[ -z "$symlink" ] || fail "gamefiles contain a symbolic link: $symlink"

generated=$(find "$SOURCE" -type f \( \
    -name '*.svtex' -o \
    -name 'Stardew Valley.XmlSerializers.dll' -o \
    -name 'MonoGame.Framework.XmlSerializers.dll' -o \
    -name 'mscorlib.XmlSerializers.dll' -o \
    -name 'SVMM.MapRuntime.dll' -o \
    -name 'xTile.dll.so' -o \
    -name 'Stardew Valley.XmlSerializers.dll.so' \
    \) -print -quit)
[ -z "$generated" ] || fail "remove files left by an earlier setup: $generated"

game_sha256=$(sha256_file "$SOURCE/Stardew Valley.exe")
xtile_sha256=$(sha256_file "$SOURCE/xTile.dll")
xnb_count=$(find "$SOURCE/Content" -type f -name '*.xnb' -print | wc -l | tr -d ' ')

case "$game_sha256" in
    0cb091faf1c3ade402340641fc47bcf9a8f6e591a645f27a4c0db2fcdc966086)
        expected_xtile=889b89f06e9699f449b448ac0e9d332c1bee61488f68e590dcb48b16867b293e
        tested=1
        ;;
    *)
        if [ "$ALLOW_UNTESTED" != 1 ]; then
            fail 'unsupported Stardew Valley compatibility build'
        fi
        expected_xtile=
        tested=0
        ;;
esac
if [ "$tested" = 1 ] && [ "$xtile_sha256" != "$expected_xtile" ]; then
    fail 'xTile.dll does not match Stardew Valley.exe'
fi
if [ "$xnb_count" != "$EXPECTED_XNB_COUNT" ]; then
    fail "incomplete Content directory: found $xnb_count XNB files, expected $EXPECTED_XNB_COUNT"
fi

if [ "$tested" = 1 ]; then
    echo "Game files verified: $SOURCE"
else
    echo "WARNING: continuing with an untested Stardew Valley build" >&2
    echo "Stardew Valley.exe SHA-256: $game_sha256" >&2
    echo "xTile.dll SHA-256: $xtile_sha256" >&2
    echo "Game file layout checked: $SOURCE"
fi
