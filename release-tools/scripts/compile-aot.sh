#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: $0 <prepared game directory>" >&2
    exit 2
fi

ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
. "$ROOT/scripts/common.sh"
GAME_DIR=$(CDPATH='' cd -- "$1" && pwd)
AOT="$ROOT/tools/aot"
ASSEMBLY="$GAME_DIR/gamedata/Stardew Valley.XmlSerializers.dll"
SIDECAR="$ASSEMBLY.so"
MARKER="$GAME_DIR/_svmm-profile-aot.txt"
DOCKER_IMAGE=${SVMM_DOCKER_IMAGE:-debian:buster}
DOCKER_PLATFORM=${SVMM_DOCKER_PLATFORM:-}

require_hash() {
    file=$1
    expected=$2
    if [ ! -f "$file" ] || [ "$(sha256_file "$file")" != "$expected" ]; then
        echo "ERROR: AOT preparation file is missing or damaged: $file" >&2
        exit 1
    fi
}

if ! command -v docker >/dev/null 2>&1; then
    echo "ERROR: Docker is required to prepare the ARM game files" >&2
    exit 1
fi

assembly_hash=$(sha256_file "$ASSEMBLY")
framework_hash=$(sha256_file "$GAME_DIR/dlls/MonoGame.Framework.dll")
game_hash=$(sha256_file "$GAME_DIR/gamedata/Stardew Valley.exe")
expected_assembly=b5ebcde94942e6214fde5cf1500feb935d546e3b991aae769574ab822f267998
expected_framework=ba855757c6f12ef44ed168d56c370994e8aecfb4fc810458ec6ab04a61628be0
expected_game=38c3f26622f3ee792fa745ecb412562dfedce672007ed9a7b56d1576d59e5004
expected_text=cafc1f2304f8adda8316800f8d3fd62ba9ac5e795feb23433a0ebe6e2c19466c
expected_rodata=c57d3e7fa4136f5d7adab901ebcbbe8b6b55035e922c3f7d09a91b7c889c045c

if [ "$assembly_hash" != "$expected_assembly" ] || \
   [ "$framework_hash" != "$expected_framework" ] || \
   [ "$game_hash" != "$expected_game" ]; then
    echo "ERROR: prepared game files do not match this release" >&2
    echo "serializer_sha256=$assembly_hash" >&2
    echo "framework_sha256=$framework_hash" >&2
    echo "game_sha256=$game_hash" >&2
    exit 1
fi
require_hash "$GAME_DIR/mono/lib/mono/4.5/mscorlib.dll" \
    383236a2a58e3b1506338f602b81d2c73ede7470c56a0727371d2b1e8ffdbd15
require_hash "$AOT/bin/mono-sgen-profilefix" \
    a73c86c3d755e6246badf14f656bb465224825e60d5304952b5740d94adcf954
require_hash "$AOT/lib/libmono-llvm.so.0.0.0" \
    bc7a047dc56e2182f1fa1eb9987dcad702cdcd545a9a7b894a18f5dba52ece87
require_hash "$AOT/llvm/bin/opt" \
    1dc58e7e0dfa8227f4b356901afcdc843072f40130b785d4c25628203aacb11a
require_hash "$AOT/llvm/bin/llc" \
    b88c3f2f509e585a11442db7f94390948825eb62e44ce37e34703ea8d727fcc6
require_hash "$AOT/profile/pressure-safe.aotprofile" \
    6ebb3145af2264e1d848110e17b12fe6f745356e97c1ce693c5af518c5837d89

WORK=$(make_temp_dir stardew-miyoo-aot)
CONTAINER=
cleanup() {
    if [ -n "$CONTAINER" ]; then
        docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
    fi
    rm -rf "$WORK"
}
trap cleanup EXIT INT TERM

echo "Compiling the ARM serializer. This usually takes two to three minutes."
if [ -n "$DOCKER_PLATFORM" ]; then
    set -- --platform "$DOCKER_PLATFORM"
else
    set --
fi
if ! docker run --rm "$@" --log-driver none "$DOCKER_IMAGE" \
    getent ahostsv4 archive.debian.org >/dev/null 2>&1; then
    echo "Docker DNS failed; retrying with public resolvers."
    set -- "$@" --dns 1.1.1.1 --dns 8.8.8.8
fi
CONTAINER=$(docker create "$@" --log-driver none \
    -v "$AOT:/aot:ro" \
    -v "$GAME_DIR:/game:ro" \
    "$DOCKER_IMAGE" sh -c '
set -eu
export DEBIAN_FRONTEND=noninteractive
export DEBCONF_NOWARNINGS=yes
dpkg --add-architecture armhf
printf "%s\n" "Acquire::Check-Valid-Until false;" \
    >/etc/apt/apt.conf.d/99no-check-valid
printf "%s\n" \
    "deb [trusted=yes] http://archive.debian.org/debian buster main" \
    >/etc/apt/sources.list
apt-get update >/dev/null
apt-get install -y --no-install-recommends \
    binutils-arm-linux-gnueabihf \
    gcc-arm-linux-gnueabihf \
    libc6-dev-armhf-cross \
    qemu-user-static \
    time >/dev/null 2>/tmp/apt-install.err || {
        cat /tmp/apt-install.err >&2
        exit 1
    }
sed "/^debconf: delaying package configuration, since apt-utils is not installed$/d" \
    /tmp/apt-install.err >&2

mkdir -p /armhf/packages /output/llvm-wrapper /output/tmp /output/sections
chown _apt:root /armhf/packages
cd /armhf/packages
apt-get download libc6:armhf libgcc1:armhf libstdc++6:armhf zlib1g:armhf \
    >/dev/null
for package in ./*.deb; do
    dpkg-deb -x "$package" /armhf
done

printf "%s\n" "#!/bin/sh" \
    "exec /usr/bin/qemu-arm-static -L /armhf /aot/llvm/bin/opt \"\$@\"" \
    >/output/llvm-wrapper/opt
printf "%s\n" "#!/bin/sh" \
    "exec /usr/bin/qemu-arm-static -L /armhf /aot/llvm/bin/llc \"\$@\"" \
    >/output/llvm-wrapper/llc
chmod +x /output/llvm-wrapper/opt /output/llvm-wrapper/llc

export MONO_PATH=/game/mono/lib/mono/4.5:/game/dlls:/game/gamedata:/aot/lib/mono/4.5
export MONO_CFG_DIR=/aot/etc
export LD_LIBRARY_PATH=/aot/lib
export TMPDIR=/output/tmp
cd /output
/usr/bin/time -v -o compile.time.txt \
    /usr/bin/qemu-arm-static -L /armhf /aot/bin/mono-sgen-profilefix -O=all \
    --aot="llvm,profile=/aot/profile/pressure-safe.aotprofile,profile-only-strict,stats,print-skipped-methods,mtriple=armv7-linux-gnueabihf,llvm-path=/output/llvm-wrapper/,tool-prefix=arm-linux-gnueabihf-,llvmopts=-O3 -mcpu=cortex-a7,outfile=/output/Stardew Valley.XmlSerializers.dll.so" \
    "/game/gamedata/Stardew Valley.XmlSerializers.dll" \
    >compile.log 2>compile.err

grep -q "Compiled: 166/166 (100%), LLVM: 158 (95%)" compile.log
arm-linux-gnueabihf-readelf -h "Stardew Valley.XmlSerializers.dll.so" \
    | grep -q "Machine:.*ARM"
arm-linux-gnueabihf-objcopy \
    --dump-section .text=/output/sections/text.bin \
    "Stardew Valley.XmlSerializers.dll.so"
arm-linux-gnueabihf-objcopy \
    --dump-section .rodata=/output/sections/rodata.bin \
    "Stardew Valley.XmlSerializers.dll.so"
')
docker start -a "$CONTAINER"
docker cp "$CONTAINER:/output/." "$WORK"
docker rm "$CONTAINER" >/dev/null
CONTAINER=

text_hash=$(sha256_file "$WORK/sections/text.bin")
rodata_hash=$(sha256_file "$WORK/sections/rodata.bin")
if [ "$text_hash" != "$expected_text" ] || \
   [ "$rodata_hash" != "$expected_rodata" ]; then
    echo "ERROR: generated ARM code does not match the tested build" >&2
    echo "text_sha256=$text_hash" >&2
    echo "rodata_sha256=$rodata_hash" >&2
    exit 1
fi

sidecar_hash=$(sha256_file "$WORK/Stardew Valley.XmlSerializers.dll.so")
cp "$WORK/Stardew Valley.XmlSerializers.dll.so" "$SIDECAR.tmp"
mv "$SIDECAR.tmp" "$SIDECAR"
cat > "$MARKER.tmp" <<EOF
format=svmm-profile-aot-package-v1
mode=game-serializer-only
assembly=gamedata/Stardew Valley.XmlSerializers.dll
assembly_sha256=$assembly_hash
sidecar=gamedata/Stardew Valley.XmlSerializers.dll.so
sidecar_sha256=$sidecar_hash
text_sha256=$text_hash
rodata_sha256=$rodata_hash
profile_sha256=6ebb3145af2264e1d848110e17b12fe6f745356e97c1ce693c5af518c5837d89
exclusion_policy_sha256=579f167f1eab51610e44da2ffe40a8da0294a94ba1ce5ce79544f21e5aa06b0c
toolchain_manifest_sha256=1d595b646fcf92b96819c73ad8b3c90d5624464a14b99d71d7db6e621f0d7253
profile_only_mode=profile-only-strict
compiled_methods=166
llvm_methods=158
compiler_sha256=a73c86c3d755e6246badf14f656bb465224825e60d5304952b5740d94adcf954
loaded_llvm_sha256=bc7a047dc56e2182f1fa1eb9987dcad702cdcd545a9a7b894a18f5dba52ece87
mono_commit=5266e6a8f107d9b91a6073e4fd1ef4eb1ac7ac6d
llvm_commit=c97510286a58f9aaa116fcfdb8b693d5f61910d2
EOF
mv "$MARKER.tmp" "$MARKER"

echo "ARM serializer ready."
