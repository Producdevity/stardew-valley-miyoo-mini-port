#!/bin/sh
set -eu

if [ "$#" -ne 3 ]; then
  echo "usage: $0 ARTIFACT_DIR OUTPUT_DIR RELEASE_TAG" >&2
  exit 2
fi

artifact_dir=$1
output_dir=$2
release_tag=$3
version=${release_tag#v}

if [ "$version" = "$release_tag" ] || [ -z "$version" ]; then
  echo "invalid release tag: $release_tag" >&2
  exit 2
fi

asset_prefix="stardew-valley-miyoo-mini-setup-v$version"
mkdir -p "$output_dir"

installer_list="$output_dir/.installers"
trap 'rm -f "$installer_list"' EXIT HUP INT TERM
find "$artifact_dir" -type f \
  \( -name '*.AppImage' -o -name '*.deb' -o -name '*.dmg' -o -name '*.exe' \) \
  -print > "$installer_list"

installer_count=$(wc -l < "$installer_list" | tr -d ' ')
if [ "$installer_count" -ne 7 ]; then
  echo "expected 7 setup files, found $installer_count" >&2
  exit 1
fi

while IFS= read -r installer; do
  name=${installer##*/}
  case "$name" in
    *_aarch64.AppImage) output_name="$asset_prefix-linux-arm64.AppImage" ;;
    *_amd64.AppImage) output_name="$asset_prefix-linux-x64.AppImage" ;;
    *_arm64.deb) output_name="$asset_prefix-linux-arm64.deb" ;;
    *_amd64.deb) output_name="$asset_prefix-linux-x64.deb" ;;
    *_aarch64.dmg) output_name="$asset_prefix-macos-arm64.dmg" ;;
    *_x64.dmg) output_name="$asset_prefix-macos-x64.dmg" ;;
    *_x64-setup.exe) output_name="$asset_prefix-windows-x64.exe" ;;
    *)
      echo "unrecognized setup filename: $name" >&2
      exit 1
      ;;
  esac

  if [ -e "$output_dir/$output_name" ]; then
    echo "duplicate setup file: $output_name" >&2
    exit 1
  fi
  cp "$installer" "$output_dir/$output_name"
done < "$installer_list"
