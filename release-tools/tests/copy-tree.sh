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

mkdir -p "$WORK/source/bin" "$WORK/target"
printf '#!/bin/sh\nexit 0\n' >"$WORK/source/bin/tool"
chmod 755 "$WORK/source/bin/tool"
touch -t 202001010000 "$WORK/source/bin/tool"

copy_tree "$WORK/source" "$WORK/target"

[ -x "$WORK/target/bin/tool" ] || fail 'copy_tree lost the executable bit'
[ "$(cat "$WORK/target/bin/tool")" = "$(cat "$WORK/source/bin/tool")" ] || \
    fail 'copy_tree changed file contents'

source_mtime=$(date -r "$WORK/source/bin/tool" +%s)
target_mtime=$(date -r "$WORK/target/bin/tool" +%s)
[ "$source_mtime" != "$target_mtime" ] || \
    fail 'copy_tree restored source timestamps'
