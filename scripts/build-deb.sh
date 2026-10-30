#!/usr/bin/env bash
set -Eeuo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
binary_dir=$(cd -- "${1:-$root/target/release}" && pwd -P)
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$root/Cargo.toml" | head -n 1)
output="$root/target/dist"
mkdir -p "$output"
work=$(mktemp -d "$output/.deb.XXXXXXXX")
trap 'rm -rf -- "$work"' EXIT
mkdir "$work/source"
cp -R "$root/debian" "$root/packaging" "$work/source/"
sed -i '/"uuid":/a\  "version-name": "'"$version"'",' "$work/source/packaging/gnome-extension/metadata.json"
mkdir "$work/source/assets"
cp -R "$root/assets/linux" "$work/source/assets/"
cat > "$work/source/debian/changelog" <<EOF
nid (${version/-/~}) unstable; urgency=medium

  * Package upstream release $version.

 -- Fabricio Dematte <demfabris@gmail.com>  $(date -R)
EOF
(
    cd "$work/source"
    NID_BIN_DIR="$binary_dir" dpkg-buildpackage --build=binary --no-sign
)
arch=$(dpkg --print-architecture)
install -m 0644 "$work/"nid_*_"$arch.deb" "$output/nid_${version}_${arch}.deb"
printf 'Created %s/nid_%s_%s.deb\n' "$output" "$version" "$arch"
