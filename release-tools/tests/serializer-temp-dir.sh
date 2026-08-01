#!/bin/sh
set -eu

# sgen must run in a writable Unix temp directory on every host platform.

ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
. "$ROOT/scripts/common.sh"

TMPDIR='C:\Users\test\Temp'
path=$(make_temp_dir stardew-miyoo-test)
case "$path" in
    /tmp/stardew-miyoo-test.*) ;;
    *) fail "unexpected temp path: $path" ;;
esac
rmdir "$path"

base=$(mktemp -d /tmp/stardew-miyoo-test-base.XXXXXX)
TMPDIR=$base
path=$(make_temp_dir stardew-miyoo-test)
case "$path" in
    "$base"/stardew-miyoo-test.*) ;;
    *) fail "TMPDIR was not used: $path" ;;
esac

# The child shell expands these variables after run_in_temp_dir sets them.
# shellcheck disable=SC2016
result=$(run_in_temp_dir "$path" sh -c 'printf "%s\n%s\n%s\n%s\n" "$PWD" "$TMPDIR" "$TMP" "$TEMP"')
expected=$(printf "%s\n%s\n%s\n%s" "$path" "$path" "$path" "$path")
[ "$result" = "$expected" ] || fail 'temporary command environment is incorrect'

rmdir "$path" "$base"
