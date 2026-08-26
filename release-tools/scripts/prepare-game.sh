#!/bin/sh
set -eu

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
    echo "usage: $0 <gamefiles directory> [OnionOS package output]" >&2
    exit 2
fi

ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
SOURCE=$1
OUT_REQUEST=${2:-$ROOT/OnionOS-package}
TEMPLATE="$ROOT/runtime-template"
TOOLS="$ROOT/tools/managed"
. "$ROOT/scripts/common.sh"

for command in mono sgen tar; do
    need "$command"
done
for required in \
    "$TEMPLATE/Roms/PORTS/Games/Stardew Valley for Miyoo Mini" \
    "$TOOLS/AssemblyRelinker.exe" \
    "$TOOLS/Mono.Cecil.dll" \
    "$TOOLS/PackageVerifier.exe" \
    "$TOOLS/XnbTextureBake.exe" \
    "$TOOLS/SVMM.MapRuntime.dll" \
    "$ROOT/scripts/compile-aot.sh" \
    "$ROOT/tools/aot/bin/mono-sgen-profilefix" \
    "$ROOT/tools/aot/profile/pressure-safe.aotprofile"
do
    [ -e "$required" ] || fail "release file is missing: $required"
done

SOURCE=$(CDPATH='' cd -- "$SOURCE" && pwd)
[ ! -L "$OUT_REQUEST" ] || fail "output must not be a symbolic link: $OUT_REQUEST"
OUT_LEAF=$(basename -- "$OUT_REQUEST")
case "$OUT_LEAF" in
    ''|.|..|/) fail 'output must name a normal directory' ;;
esac
OUT_PARENT_REQUEST=$(dirname -- "$OUT_REQUEST")
mkdir -p "$OUT_PARENT_REQUEST"
OUT_PARENT=$(CDPATH='' cd -- "$OUT_PARENT_REQUEST" && pwd)
if [ -d "$OUT_REQUEST" ]; then
    OUT=$(CDPATH='' cd -- "$OUT_REQUEST" && pwd)
else
    OUT="$OUT_PARENT/$OUT_LEAF"
fi
TEMPLATE=$(CDPATH='' cd -- "$TEMPLATE" && pwd)

case "$OUT" in /|//) fail 'output must not be the filesystem root' ;; esac

reject_overlap() {
    first=$1
    second=$2
    description=$3
    case "$first" in "$second"|"$second"/*) fail "$description overlap" ;; esac
    case "$second" in "$first"|"$first"/*) fail "$description overlap" ;; esac
}

reject_overlap "$SOURCE" "$OUT" 'gamefiles and output directories'
reject_overlap "$TEMPLATE" "$OUT" 'runtime template and output directories'
[ ! -e "$OUT" ] || [ -d "$OUT" ] || fail "output is not a directory: $OUT"

STAGING="$OUT.tmp.$$"
BACKUP="$OUT.previous.$$"
BAKED="$OUT.baked.$$"
SERIALIZER_TMP=

cleanup() {
    if [ -n "$SERIALIZER_TMP" ]; then
        rm -rf "$SERIALIZER_TMP"
    fi
    rm -rf "$STAGING" "$BAKED"
    if [ ! -e "$OUT" ] && [ -e "$BACKUP" ]; then
        mv "$BACKUP" "$OUT"
    else
        rm -rf "$BACKUP"
    fi
}
trap cleanup EXIT INT TERM

relink_assembly() {
    target=$1
    relinked="$target.relinked"
    verified="$target.relinked.verify"
    rm -f "$relinked" "$verified"
    MONO_PATH="$TOOLS${MONO_PATH:+:$MONO_PATH}" \
        mono "$TOOLS/AssemblyRelinker.exe" "$target" "$relinked"
    MONO_PATH="$TOOLS${MONO_PATH:+:$MONO_PATH}" \
        mono "$TOOLS/AssemblyRelinker.exe" "$relinked" "$verified"
    if ! cmp -s "$relinked" "$verified"; then
        mv "$verified" "$relinked"
        MONO_PATH="$TOOLS${MONO_PATH:+:$MONO_PATH}" \
            mono "$TOOLS/AssemblyRelinker.exe" "$relinked" "$verified"
        if ! cmp -s "$relinked" "$verified"; then
            fail "relinker output did not stabilize: $target"
        fi
    fi
    mv "$relinked" "$target"
    rm -f "$verified"
}

build_serializers() {
    game_exe=$1
    game_dir=$(CDPATH='' cd -- "$(dirname -- "$game_exe")" && pwd)
    dll_dir=$(CDPATH='' cd -- "$game_dir/../dlls" && pwd)
    SERIALIZER_TMP=$(make_temp_dir stardew-miyoo-serializer)

    MONO_PATH="$dll_dir:$game_dir${MONO_PATH:+:$MONO_PATH}" \
    run_in_temp_dir "$SERIALIZER_TMP" sgen --force --silent --assembly:"$game_exe" \
        --type:StardewValley.SaveGame \
        --type:StardewValley.Farmer \
        --type:StardewValley.GameLocation \
        --type:StardewValley.Quests.DescriptionElement \
        --type:'StardewValley.SaveMigrations.SaveMigrator_1_6+LegacyDescriptionElement' \
        --type:StardewValley.FarmAnimal \
        --type:StardewValley.Locations.MineInfo \
        --type:StardewValley.Inventories.Inventory \
        --type:StardewValley.Options \
        --type:StardewValley.Network.LocationWeather \
        --type:StardewValley.Item \
        --type:StardewValley.TerrainFeatures.TerrainFeature \
        --type:StardewValley.StartupPreferences \
        --type:StardewValley.Object \
        --type:StardewValley.Friendship \
        --type:'StardewValley.SerializableDictionary`2[[System.String, mscorlib],[System.Int32, mscorlib]]' \
        --out:"$SERIALIZER_TMP"
    mv "$SERIALIZER_TMP/Stardew Valley.XmlSerializers.dll" "$game_dir/"

    MONO_PATH="$dll_dir:$game_dir${MONO_PATH:+:$MONO_PATH}" \
    run_in_temp_dir "$SERIALIZER_TMP" sgen --force --silent \
        --assembly:"$dll_dir/MonoGame.Framework.dll" \
        --type:Microsoft.Xna.Framework.Vector2 \
        --out:"$SERIALIZER_TMP"
    mv "$SERIALIZER_TMP/MonoGame.Framework.XmlSerializers.dll" "$game_dir/"

    mono_bin=$(command -v mono)
    mono_prefix=$(CDPATH='' cd -- "$(dirname -- "$mono_bin")/.." && pwd)
    mscorlib="$mono_prefix/lib/mono/4.5/mscorlib.dll"
    [ -f "$mscorlib" ] || fail "Mono mscorlib.dll is missing: $mscorlib"
    run_in_temp_dir "$SERIALIZER_TMP" sgen --force --silent \
        --assembly:"$mscorlib" \
        --type:'System.Boolean[]' \
        --type:'System.Int32[]' \
        --out:"$SERIALIZER_TMP"
    mv "$SERIALIZER_TMP/mscorlib.XmlSerializers.dll" "$game_dir/"

    for companion in \
        "$game_dir/Stardew Valley.XmlSerializers.dll" \
        "$game_dir/MonoGame.Framework.XmlSerializers.dll" \
        "$game_dir/mscorlib.XmlSerializers.dll"
    do
        [ -s "$companion" ] || fail "serializer was not generated: $companion"
    done
    rm -rf "$SERIALIZER_TMP"
    SERIALIZER_TMP=
}

run_serializer_checks() {
    game_dir=$1
    game_exe="$game_dir/gamedata/Stardew Valley.exe"
    mono_path="$game_dir/dlls:$game_dir/gamedata"
    MONO_PATH="$mono_path" mono "$TOOLS/SerializerCheck.exe" \
        "$game_exe" 'Stardew Valley.XmlSerializers'
    for check in \
        RuntimeSerializerCheck.exe \
        ObjectSaveCheck.exe \
        ItemSaveCheck.exe \
        NetworkSaveCheck.exe
    do
        MONO_PATH="$mono_path" mono "$TOOLS/$check" "$game_exe"
    done
}

"$ROOT/scripts/check-gamefiles.sh" "$SOURCE"
source_tree_before=$(hash_tree "$SOURCE")
source_game_hash=$(sha256_file "$SOURCE/Stardew Valley.exe")
case "$source_game_hash" in
    0cb091faf1c3ade402340641fc47bcf9a8f6e591a645f27a4c0db2fcdc966086)
        game_version=1.6.15.24356
        tested_build=1
        ;;
    *)
        if [ "${SVMM_ALLOW_UNTESTED_GAMEFILES:-0}" != 1 ]; then
            fail 'unsupported Stardew Valley compatibility build'
        fi
        game_version=untested
        tested_build=0
        ;;
esac

rm -rf "$STAGING" "$BACKUP" "$BAKED"
copy_tree "$TEMPLATE" "$STAGING"
GAME_DIR="$STAGING/Roms/PORTS/Games/Stardew Valley for Miyoo Mini"
rm -rf "$GAME_DIR/gamedata" "$GAME_DIR/savedata"
mkdir -p "$GAME_DIR/gamedata" "$GAME_DIR/savedata"
copy_tree "$SOURCE" "$GAME_DIR/gamedata"
rm -rf \
    "$GAME_DIR/gamedata/.DepotDownloader" \
    "$GAME_DIR/gamedata/__MACOSX" \
    "$GAME_DIR/gamedata/_CommonRedist"
find "$GAME_DIR/gamedata" -type f \( \
    -name '.DS_Store' -o -name '._*' -o -name 'Thumbs.db' -o -name '.gitkeep' \
    \) -delete

# shellcheck source=release-tools/scripts/relink-profile.sh
. "$ROOT/scripts/relink-profile.sh"
svmm_write_relink_profile "$GAME_DIR/_svmm-relink-profile.txt"
printf '0\n' > "$GAME_DIR/_svmm-managed-aot-mode.txt"
cat > "$GAME_DIR/_svmm-profile-aot.txt" <<'PROFILE_AOT'
format=svmm-profile-aot-package-v1
mode=none
PROFILE_AOT
rm -f \
    "$GAME_DIR/gamedata/xTile.dll.so" \
    "$GAME_DIR/gamedata/Stardew Valley.XmlSerializers.dll.so"

GAME_EXE="$GAME_DIR/gamedata/Stardew Valley.exe"
relink_assembly "$GAME_EXE"
sha256_file "$GAME_EXE" > "$GAME_DIR/_svmm-game-assembly.sha256"
build_serializers "$GAME_EXE"
relink_assembly "$GAME_DIR/gamedata/Stardew Valley.XmlSerializers.dll"
run_serializer_checks "$GAME_DIR"
relink_assembly "$GAME_DIR/gamedata/xTile.dll"
cp "$TOOLS/SVMM.MapRuntime.dll" "$GAME_DIR/gamedata/SVMM.MapRuntime.dll"
if [ "$tested_build" = 1 ]; then
    "$ROOT/scripts/compile-aot.sh" "$GAME_DIR"
else
    echo "WARNING: ARM serializer AOT is disabled for this untested build" >&2
fi

mkdir -p "$BAKED"
MONO_PATH="$GAME_DIR/dlls:$GAME_DIR/gamedata:$TOOLS${MONO_PATH:+:$MONO_PATH}" \
    mono "$TOOLS/XnbTextureBake.exe" --all \
    "$GAME_DIR/gamedata/Content" "$BAKED" auto nearest
copy_tree "$BAKED" "$GAME_DIR/gamedata/Content"

if [ "$tested_build" = 1 ]; then
    cat > "$GAME_DIR/_svmm-preparation-receipt.txt" <<EOF
version=1
game_version=$game_version
source_tree_sha256=$source_tree_before
EOF
else
    cat > "$GAME_DIR/_svmm-preparation-receipt.txt" <<EOF
version=1
game_version=untested
source_game_sha256=$source_game_hash
source_tree_sha256=$source_tree_before
EOF
fi

source_tree_after=$(hash_tree "$SOURCE")
[ "$source_tree_after" = "$source_tree_before" ] || \
    fail 'source gamefiles changed during preparation'

MONO_PATH="$TOOLS${MONO_PATH:+:$MONO_PATH}" \
    mono "$TOOLS/PackageVerifier.exe" --prepared-game "$STAGING"

if [ -e "$OUT" ]; then
    mv "$OUT" "$BACKUP"
fi
mv "$STAGING" "$OUT"
rm -rf "$BACKUP" "$BAKED"
trap - EXIT INT TERM

echo "Prepared OnionOS package: $OUT"
echo "The original game files were not changed."
