#!/bin/sh
set -eu

appimage=${1:?usage: check-appimage.sh FILE.AppImage}
appimage=$(cd "$(dirname "$appimage")" && pwd)/$(basename "$appimage")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

(
    cd "$work"
    "$appimage" --appimage-extract >/dev/null
)

bundled=$(find "$work/squashfs-root/usr" \( -type f -o -type l \) \
    -name 'libwayland-*.so*' -print)
[ -z "$bundled" ] || {
    echo "AppImage contains host-sensitive Wayland libraries:" >&2
    echo "$bundled" >&2
    exit 1
}

echo "AppImage library check passed: $appimage"
