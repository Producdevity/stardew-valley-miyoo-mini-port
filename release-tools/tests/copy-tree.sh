#!/bin/sh
set -eu

ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
. "$ROOT/scripts/common.sh"

WORK=$(mktemp -d /tmp/stardew-miyoo-copy-tree.XXXXXX)
cleanup() {
    chmod -R u+w "$WORK" 2>/dev/null || true
    rm -rf "$WORK"
}
trap cleanup EXIT INT TERM

mkdir -p "$WORK/source/bin" "$WORK/source/empty" "$WORK/target/bin"
printf '#!/bin/sh\nexit 0\n' >"$WORK/source/bin/tool"
printf 'hidden\n' >"$WORK/source/.hidden"
ln -s bin/tool "$WORK/source/tool-link"
chmod 755 "$WORK/source/bin/tool"
touch -t 202001010000 "$WORK/source/bin/tool"
chmod 700 "$WORK/target" "$WORK/target/bin"

copy_tree "$WORK/source" "$WORK/target"

[ -x "$WORK/target/bin/tool" ] || fail 'copy_tree lost the executable bit'
[ "$(cat "$WORK/target/bin/tool")" = "$(cat "$WORK/source/bin/tool")" ] || \
    fail 'copy_tree changed file contents'

[ "$(find "$WORK/target/bin/tool" -newer "$WORK/source/bin/tool")" = "$WORK/target/bin/tool" ] || \
    fail 'copy_tree restored source timestamps'
[ "$(find "$WORK/target" -prune -perm 0700)" = "$WORK/target" ] || \
    fail 'copy_tree changed the destination directory permissions'
[ "$(find "$WORK/target/bin" -prune -perm 0700)" = "$WORK/target/bin" ] || \
    fail 'copy_tree changed existing subdirectory permissions'
[ -d "$WORK/target/empty" ] || fail 'copy_tree dropped an empty directory'
cmp "$WORK/source/.hidden" "$WORK/target/.hidden"
[ -L "$WORK/target/tool-link" ] || fail 'copy_tree dereferenced a symbolic link'
[ "$(readlink "$WORK/target/tool-link")" = bin/tool ] || fail 'copy_tree changed a symbolic link'

if (copy_tree "$WORK/missing" "$WORK/target") 2>/dev/null; then
    fail 'copy_tree ignored a missing source directory'
fi
