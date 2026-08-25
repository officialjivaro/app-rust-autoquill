#!/usr/bin/env bash
set -euo pipefail

version="${1:?usage: package-macos.sh VERSION OUTPUT_DIRECTORY}"
output_dir="${2:?usage: package-macos.sh VERSION OUTPUT_DIRECTORY}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
arm_binary="$repo_root/target/aarch64-apple-darwin/release-size/autoquill"
intel_binary="$repo_root/target/x86_64-apple-darwin/release-size/autoquill"
work_dir="$(mktemp -d)"
app="$work_dir/AutoQuill.app"
iconset="$work_dir/AutoQuill.iconset"
dmg_root="$work_dir/dmg-root"
trap 'rm -rf "$work_dir"' EXIT

test -x "$arm_binary"
test -x "$intel_binary"
mkdir -p "$output_dir" "$app/Contents/MacOS" "$app/Contents/Resources" "$iconset" "$dmg_root"

lipo -create "$arm_binary" "$intel_binary" -output "$app/Contents/MacOS/AutoQuill"
lipo "$app/Contents/MacOS/AutoQuill" -verify_arch arm64 x86_64
if grep -aFq "$HOME" "$app/Contents/MacOS/AutoQuill"; then
  echo "The macOS release contains the runner home path; rebuild it with path remapping." >&2
  exit 1
fi
chmod 755 "$app/Contents/MacOS/AutoQuill"
cp "$repo_root/packaging/macos/Info.plist" "$app/Contents/Info.plist"
short_version="${version%%-*}"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $short_version" "$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $short_version" "$app/Contents/Info.plist"
cp "$repo_root/THIRD_PARTY_NOTICES.md" "$app/Contents/Resources/THIRD_PARTY_NOTICES.md"
cp "$repo_root/DEPENDENCY_LICENSES.md" "$app/Contents/Resources/DEPENDENCY_LICENSES.md"

for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$repo_root/assets/icon-master.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" "$repo_root/assets/icon-master.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/AutoQuill.icns"
plutil -lint "$app/Contents/Info.plist"
otool -L "$app/Contents/MacOS/AutoQuill"

AUTOQUILL_SMOKE_TEST=1 "$app/Contents/MacOS/AutoQuill"

archive="$output_dir/AutoQuill-${version}-macos-universal2-unsigned.zip"
dmg="$output_dir/AutoQuill-${version}-macos-universal2-unsigned.dmg"
ditto -c -k --sequesterRsrc --keepParent "$app" "$archive"
ditto "$app" "$dmg_root/AutoQuill.app"
ln -s /Applications "$dmg_root/Applications"
hdiutil create -quiet -fs HFS+ -srcfolder "$dmg_root" -volname "AutoQuill ${version}" "$dmg"
hdiutil verify -quiet "$dmg"

archive_hash="$(shasum -a 256 "$archive" | awk '{print $1}')"
dmg_hash="$(shasum -a 256 "$dmg" | awk '{print $1}')"
archive_size="$(stat -f %z "$archive")"
dmg_size="$(stat -f %z "$dmg")"
printf '%s  %s\n%s  %s\n' "$archive_hash" "$(basename "$archive")" "$dmg_hash" "$(basename "$dmg")" > "$output_dir/SHA256SUMS-macos.txt"
printf '{\n  "version": "%s",\n  "platform": "macos-universal2",\n  "verification": "logical-and-native-runner-only",\n  "signed": false,\n  "artifacts": [\n    {"name": "%s", "bytes": %s, "sha256": "%s"},\n    {"name": "%s", "bytes": %s, "sha256": "%s"}\n  ]\n}\n' \
  "$version" "$(basename "$archive")" "$archive_size" "$archive_hash" "$(basename "$dmg")" "$dmg_size" "$dmg_hash" > "$output_dir/manifest-macos.json"
