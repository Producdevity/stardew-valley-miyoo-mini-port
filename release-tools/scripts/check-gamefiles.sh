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
tree_sha256=$(hash_tree "$SOURCE")

case "$game_sha256" in
    505d343f04420186ba2b611bcc5d256eff554451f55a6b37f3454362d5e03656)
        expected_xtile=a05a1123aa3abb8c68ec2589649dfac724dd3cc52a2e0d812f04ffab794a7be5
        expected_tree=fdb83eb53853ebd8864899515d2a33942f5fd22ea4025f501ae167488176d9ea
        expected_depot_tree=
        ;;
    0cb091faf1c3ade402340641fc47bcf9a8f6e591a645f27a4c0db2fcdc966086)
        expected_xtile=889b89f06e9699f449b448ac0e9d332c1bee61488f68e590dcb48b16867b293e
        expected_tree=72437db8eb72d73f5c4834f2e6adec63a8a96af3c504df323cb852fd2703492d
        # Depot 413151 omits Steam's shared Windows redistributable installers.
        expected_depot_tree=b45f89323e6a5de628f64183f1f105156e6cef122f6df998e4ba07108599e633
        ;;
    *) fail 'unsupported Stardew Valley compatibility build' ;;
esac
if [ "$xtile_sha256" != "$expected_xtile" ]; then
    fail 'xTile.dll does not match Stardew Valley.exe'
fi
if [ "$xnb_count" != "$EXPECTED_XNB_COUNT" ]; then
    fail "incomplete Content directory: found $xnb_count XNB files, expected $EXPECTED_XNB_COUNT"
fi
if [ "$tree_sha256" != "$expected_tree" ] && \
   [ "$tree_sha256" != "$expected_depot_tree" ]; then
    fail 'gamefiles do not match the supported compatibility release'
fi

echo "Game files verified: $SOURCE"
