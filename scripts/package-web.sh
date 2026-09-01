#!/usr/bin/env bash
set -euo pipefail

repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output_root="${OUTPUT_DIRECTORY:-$repository/dist}"
stage="$output_root/stellarion-html"
archive="$output_root/stellarion-html.zip"
wasm_bindgen_version="$(tr -d '[:space:]' < "$repository/scripts/wasm-bindgen-version.txt")"

case "$output_root" in
  "$repository"/dist|"$repository"/dist/*) ;;
  *) echo "Refusing to clean output outside $repository/dist" >&2; exit 1 ;;
esac

mkdir -p "$output_root"
rm -rf -- "$stage"
mkdir -p "$stage"

if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  cargo build --manifest-path "$repository/Cargo.toml" --profile wasm-release \
    --target wasm32-unknown-unknown --bin stellarion -j12
fi

installed_version="$(wasm-bindgen --version | sed 's/^wasm-bindgen //')"
if [[ "$installed_version" != "$wasm_bindgen_version" ]]; then
  echo "wasm-bindgen-cli $wasm_bindgen_version is required; found $installed_version" >&2
  exit 1
fi

wasm-bindgen "$repository/target/wasm32-unknown-unknown/wasm-release/stellarion.wasm" \
  --target web --out-dir "$stage" --out-name stellarion --no-typescript
cp "$repository/web/index.html" "$repository/LICENSE" "$repository/README.md" "$stage/"
cp -R "$repository/assets-runtime" "$stage/assets-runtime"

rm -f -- "$archive"
(cd "$stage" && zip -qr "$archive" .)
echo "Created $archive"
