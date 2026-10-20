#!/bin/sh
set -eu

# Exercise the release copy using an output filesystem supplied by the caller.
[ "$#" -eq 1 ] && [ -d "$1" ] || {
    echo "usage: $0 <existing output parent>" >&2
    exit 2
}
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
. "$ROOT/scripts/common.sh"

SOURCE=$(mktemp -d /tmp/stardew-miyoo-copy-output.XXXXXX)
WORK=
cleanup() {
    rm -rf "$SOURCE"
    [ -z "$WORK" ] || rm -rf "$WORK"
}
trap cleanup EXIT INT TERM
WORK=$(mktemp -d "$1/stardew-miyoo-copy-output.XXXXXX")

mkdir -p "$SOURCE/Steam game/dlls" "$SOURCE/Steam game/Content/empty"
printf '#!/bin/sh\nexit 0\n' >"$SOURCE/Steam game/dlls/tool"
printf 'game fixture\n' >"$SOURCE/Steam game/Stardew Valley.exe"
printf 'hidden fixture\n' >"$SOURCE/Steam game/.hidden"
chmod 755 "$SOURCE/Steam game/dlls/tool"
ln -s dlls/tool "$SOURCE/Steam game/tool-link"

OUT="$WORK/OnionOS package"
copy_tree "$SOURCE/Steam game" "$OUT"
cmp "$SOURCE/Steam game/Stardew Valley.exe" "$OUT/Stardew Valley.exe"
cmp "$SOURCE/Steam game/dlls/tool" "$OUT/dlls/tool"
cmp "$SOURCE/Steam game/.hidden" "$OUT/.hidden"
[ -x "$OUT/dlls/tool" ] || fail 'output executable is not executable'
[ -d "$OUT/Content/empty" ] || fail 'output lost an empty directory'
[ -L "$OUT/tool-link" ] || fail 'output lost a symbolic link'
[ "$(readlink "$OUT/tool-link")" = dlls/tool ] || fail 'output changed a symbolic link'
[ ! -e "$OUT/Steam game" ] || fail 'output contains an extra source directory'

if (copy_tree "$SOURCE/missing" "$WORK/invalid copy") 2>/dev/null; then
    fail 'output copy ignored a missing source directory'
fi
