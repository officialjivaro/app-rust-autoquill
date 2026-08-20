#!/usr/bin/env bash
set -euo pipefail

renderer="${1:-software}"
case "$renderer" in
  femtovg|software) ;;
  *) echo "renderer must be femtovg or software" >&2; exit 2 ;;
esac

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target_dir="$project_root/target/size-$renderer"

cargo build \
  --manifest-path "$project_root/Cargo.toml" \
  --profile release-size \
  --no-default-features \
  --features "renderer-$renderer" \
  --target-dir "$target_dir"

executable="$target_dir/release-size/autoquill"
bytes="$(wc -c < "$executable" | tr -d ' ')"

printf 'Renderer: %s\nExecutable: %s\nBytes: %s\n' "$renderer" "$executable" "$bytes"
