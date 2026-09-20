#!/usr/bin/env bash
set -euo pipefail

version="${1:?usage: package-linux.sh VERSION OUTPUT_DIRECTORY}"
output_dir="${2:?usage: package-linux.sh VERSION OUTPUT_DIRECTORY}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output_dir="$(realpath -m "$output_dir")"
binary="$repo_root/target/release-size/autoquill"
work_dir="$(mktemp -d)"
app_dir="$work_dir/AutoQuill.AppDir"
tar_dir="$work_dir/AutoQuill-${version}-linux-x64"
tool="$work_dir/linuxdeploy-x86_64.AppImage"
icon_file="$work_dir/net.jivaro.autoquill.png"
tool_sha256="c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d"
trap 'rm -rf "$work_dir"' EXIT

test -x "$binary"
AUTOQUILL_X11_TEST=1 XDG_SESSION_TYPE=x11 cargo test --locked --no-default-features --features renderer-software --lib x11_focus_changes_stop_delivery -- --ignored --test-threads=1
if grep -aFq "$HOME" "$binary"; then
  echo "The Linux release contains the runner home path; rebuild it with path remapping." >&2
  exit 1
fi
ldconfig_cache="$work_dir/ldconfig-cache.txt"
ldconfig -p > "$ldconfig_cache"
xkbcommon_x11="$(awk '$1 == "libxkbcommon-x11.so.0" { print $NF; exit }' "$ldconfig_cache")"
if [[ -z "$xkbcommon_x11" || ! -r "$xkbcommon_x11" ]]; then
  echo "libxkbcommon-x11.so.0 is required to validate and package the X11 release." >&2
  exit 1
fi
mkdir -p "$output_dir" "$app_dir/usr/bin" "$app_dir/usr/share/doc/autoquill" "$tar_dir"
cp "$repo_root/THIRD_PARTY_NOTICES.md" "$app_dir/usr/share/doc/autoquill/THIRD_PARTY_NOTICES.md"
cp "$repo_root/DEPENDENCY_LICENSES.md" "$app_dir/usr/share/doc/autoquill/DEPENDENCY_LICENSES.md"
cp "$repo_root/packaging/linux/README.md" "$app_dir/usr/share/doc/autoquill/README.md"
cp "$repo_root/packaging/linux/README.md" "$output_dir/README-linux.md"
cp "$repo_root/THIRD_PARTY_NOTICES.md" "$output_dir/THIRD_PARTY_NOTICES.md"
cp "$repo_root/DEPENDENCY_LICENSES.md" "$output_dir/DEPENDENCY_LICENSES.md"
cp "$repo_root/assets/icon.png" "$icon_file"

raw="$output_dir/AutoQuill-${version}-linux-x64"
cp "$binary" "$raw"
chmod 755 "$raw"
cp "$raw" "$tar_dir/AutoQuill"
cp "$repo_root/THIRD_PARTY_NOTICES.md" "$tar_dir/THIRD_PARTY_NOTICES.md"
cp "$repo_root/DEPENDENCY_LICENSES.md" "$tar_dir/DEPENDENCY_LICENSES.md"
cp "$repo_root/packaging/linux/README.md" "$tar_dir/README.md"
tar -C "$work_dir" -czf "$output_dir/AutoQuill-${version}-linux-x64.tar.gz" "$(basename "$tar_dir")"

curl --fail --location --retry 3 --silent --show-error \
  https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20251107-1/linuxdeploy-x86_64.AppImage \
  --output "$tool"
printf '%s  %s\n' "$tool_sha256" "$tool" | sha256sum --check --strict
chmod 755 "$tool"

(
  cd "$work_dir"
  export ARCH=x86_64
  export APPIMAGE_EXTRACT_AND_RUN=1
  "$tool" \
    --appdir "$app_dir" \
    --executable "$binary" \
    --library "$xkbcommon_x11" \
    --desktop-file "$repo_root/packaging/linux/net.jivaro.autoquill.desktop" \
    --icon-file "$icon_file" \
    --output appimage
)
generated_appimage="$(find "$work_dir" -maxdepth 1 -name '*.AppImage' ! -name 'linuxdeploy-*' -print -quit)"
test -n "$generated_appimage"
appimage="$output_dir/AutoQuill-${version}-linux-x64.AppImage"
mv "$generated_appimage" "$appimage"
chmod 755 "$appimage"

file "$raw" "$appimage"
readelf -h "$raw"
if ldd "$raw" | grep -F 'not found'; then
  echo 'A required raw-binary dependency was not found.' >&2
  exit 1
fi
AUTOQUILL_SMOKE_TEST=1 "$raw"
APPIMAGE_EXTRACT_AND_RUN=1 AUTOQUILL_SMOKE_TEST=1 "$appimage"
(
  cd "$work_dir"
  "$appimage" --appimage-extract >/dev/null
  test -x squashfs-root/AppRun
  test -x squashfs-root/usr/bin/autoquill
  bundled_xkbcommon="$(find squashfs-root/usr/lib -name 'libxkbcommon-x11.so*' -print -quit)"
  test -n "$bundled_xkbcommon"
)

tarball="$output_dir/AutoQuill-${version}-linux-x64.tar.gz"
raw_hash="$(sha256sum "$raw" | awk '{print $1}')"
tar_hash="$(sha256sum "$tarball" | awk '{print $1}')"
appimage_hash="$(sha256sum "$appimage" | awk '{print $1}')"
raw_size="$(stat -c %s "$raw")"
tar_size="$(stat -c %s "$tarball")"
appimage_size="$(stat -c %s "$appimage")"
printf '%s  %s\n%s  %s\n%s  %s\n' \
  "$raw_hash" "$(basename "$raw")" "$tar_hash" "$(basename "$tarball")" "$appimage_hash" "$(basename "$appimage")" > "$output_dir/SHA256SUMS-linux.txt"
printf '{\n  "version": "%s",\n  "platform": "linux-x64",\n  "verification": "logical-xvfb-and-native-runner-only",\n  "wayland": "simulation-only",\n  "artifacts": [\n    {"name": "%s", "bytes": %s, "sha256": "%s"},\n    {"name": "%s", "bytes": %s, "sha256": "%s"},\n    {"name": "%s", "bytes": %s, "sha256": "%s"}\n  ]\n}\n' \
  "$version" "$(basename "$raw")" "$raw_size" "$raw_hash" "$(basename "$tarball")" "$tar_size" "$tar_hash" "$(basename "$appimage")" "$appimage_size" "$appimage_hash" > "$output_dir/manifest-linux.json"
