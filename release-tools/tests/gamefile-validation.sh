#!/bin/sh
set -eu

ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
CHECK="$ROOT/scripts/check-gamefiles.sh"
WORK=$(mktemp -d /tmp/stardew-miyoo-gamefiles.XXXXXX)
trap 'rm -rf "$WORK"' EXIT INT TERM

GAME="$WORK/game"
BIN="$WORK/bin"
mkdir -p "$GAME/Content" "$BIN"
: > "$GAME/Stardew Valley.exe"
: > "$GAME/xTile.dll"

i=1
while [ "$i" -le 3550 ]; do
    : > "$GAME/Content/fixture-$i.xnb"
    i=$((i + 1))
done

cat > "$BIN/shasum" <<'EOF'
#!/bin/sh
for path do :; done
case "$path:${SVMM_TEST_LEGACY_BUILD:-0}" in
    */Stardew\ Valley.exe:1)
        hash=505d343f04420186ba2b611bcc5d256eff554451f55a6b37f3454362d5e03656
        ;;
    */xTile.dll:1)
        hash=a05a1123aa3abb8c68ec2589649dfac724dd3cc52a2e0d812f04ffab794a7be5
        ;;
    */Stardew\ Valley.exe:0)
        hash=0cb091faf1c3ade402340641fc47bcf9a8f6e591a645f27a4c0db2fcdc966086
        ;;
    */xTile.dll:0)
        hash=889b89f06e9699f449b448ac0e9d332c1bee61488f68e590dcb48b16867b293e
        ;;
    *)
        exit 2
        ;;
esac
printf '%s  %s\n' "$hash" "$path"
EOF
chmod 755 "$BIN/shasum"

# Steam's client and DepotDownloader may include different files outside the
# game payload. Those files must not make a valid build fail validation.
: > "$GAME/harmless-extra-file.txt"
PATH="$BIN:$PATH" "$CHECK" "$GAME" >/dev/null

if SVMM_TEST_LEGACY_BUILD=1 PATH="$BIN:$PATH" "$CHECK" "$GAME" \
    >"$WORK/output" 2>&1; then
    echo "game validation accepted the retired compatibility build" >&2
    exit 1
fi
grep -q 'unsupported Stardew Valley compatibility build' "$WORK/output"

SVMM_TEST_LEGACY_BUILD=1 \
SVMM_ALLOW_UNTESTED_GAMEFILES=1 \
PATH="$BIN:$PATH" "$CHECK" "$GAME" >"$WORK/output" 2>&1
grep -q 'WARNING: continuing with an untested Stardew Valley build' "$WORK/output"
grep -q 'Stardew Valley.exe SHA-256: 505d343f' "$WORK/output"

rm "$GAME/Content/fixture-3550.xnb"
if SVMM_ALLOW_UNTESTED_GAMEFILES=1 PATH="$BIN:$PATH" "$CHECK" "$GAME" \
    >"$WORK/output" 2>&1; then
    echo "game validation accepted an incomplete Content directory" >&2
    exit 1
fi
grep -q 'found 3549 XNB files, expected 3550' "$WORK/output"

: > "$GAME/Content/fixture-3550.xnb"
: > "$GAME/Content/generated.svtex"
if PATH="$BIN:$PATH" "$CHECK" "$GAME" >"$WORK/output" 2>&1; then
    echo "game validation accepted files from an earlier setup" >&2
    exit 1
fi
grep -q 'remove files left by an earlier setup' "$WORK/output"
