#!/usr/bin/env bash
set -euo pipefail

repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
target="${TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"
channel="${CHANNEL:-}"
if [[ -z "$channel" ]]; then
  case "$target" in
    *windows*) channel="windows" ;;
    *apple-darwin*) channel="mac" ;;
    *) channel="linux" ;;
  esac
fi
if [[ ! "$channel" =~ ^[A-Za-z0-9][A-Za-z0-9_-]*$ ]]; then
  echo "Package channel must contain only letters, digits, hyphens, and underscores." >&2
  exit 1
fi

if [[ ! -f "$repository/assets-runtime/.stellarion-assets" ]]; then
  echo "Runtime assets are missing. Install KTX-Software 4.x and run 'just assets' before packaging." >&2
  exit 1
fi

source "$repository/scripts/common.sh"
prepare_package_output
stage="$output_root/stellarion-$channel"
archive="$output_root/stellarion-$channel.zip"
rm -rf -- "$stage"
mkdir -p "$stage"

if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo build --manifest-path "$repository/Cargo.toml" --release --target "$target" \
    --bin stellarion -j12
fi

executable="stellarion"
[[ "$target" == *windows* ]] && executable="stellarion.exe"
binary_path="${BINARY_PATH:-$repository/target/$target/release/$executable}"
cp "$binary_path" "$stage/"
cp -R "$repository/assets-runtime" "$stage/assets-runtime"
cp "$repository/LICENSE" "$repository/README.md" "$stage/"
rm -f -- "$archive"
(cd "$stage" && zip -qr "$archive" .)
echo "Created $archive"
