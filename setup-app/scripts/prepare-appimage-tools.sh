#!/bin/sh
set -eu

commit=b5eb8d05b4c0ed40107fe2158c5d8527f94568ef
expected=cb379f9b0733e9ad9f8bd78f8c2fa038aef2478523bb7d4c8e64ff6a1ea3501a
cache_dir=${XDG_CACHE_HOME:-"$HOME/.cache"}/tauri
plugin=$cache_dir/linuxdeploy-plugin-gtk.sh
download=$plugin.download

mkdir -p "$cache_dir"
trap 'rm -f "$download"' EXIT
curl -fsSL \
    "https://raw.githubusercontent.com/tauri-apps/linuxdeploy-plugin-gtk/$commit/linuxdeploy-plugin-gtk.sh" \
    -o "$download"
actual=$(sha256sum "$download" | cut -d ' ' -f 1)
[ "$actual" = "$expected" ] || {
    echo "linuxdeploy GTK plugin checksum mismatch" >&2
    exit 1
}

cat >> "$download" <<'EOF'

# Use the host's matching Wayland stack instead of mixing it with Ubuntu libraries.
find "$APPDIR"/usr/lib* \( -type f -o -type l \) \( \
    -name 'libwayland-client.so*' -o \
    -name 'libwayland-cursor.so*' -o \
    -name 'libwayland-egl.so*' -o \
    -name 'libwayland-server.so*' \
\) -delete
EOF

chmod 755 "$download"
mv "$download" "$plugin"
trap - EXIT
