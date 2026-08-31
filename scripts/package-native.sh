#!/usr/bin/env bash
set -euo pipefail

repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target="${TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"
channel="${CHANNEL:-}"
if [[ -z "$channel" ]]; then
  case "$target" in
    *windows*) channel="windows" ;;
    *apple-darwin*) channel="mac" ;;
    *) channel="linux" ;;
  esac
fi
output_root="${OUTPUT_DIRECTORY:-$repository/dist}"
stage="$output_root/stellarion-$channel"
archive="$output_root/stellarion-$channel.zip"

validate_public_key() {
  STELLARION_VALIDATION_KEY="$1" python3 - <<'PY'
import base64
import binascii
import json
import os
import sys

key = os.environ["STELLARION_VALIDATION_KEY"].strip()
if not key:
    sys.exit("Supabase publishable key is empty")
if key.lower().startswith("sb_secret_"):
    sys.exit("Refusing to package a Supabase secret key")
parts = key.split(".")
if len(parts) == 3:
    try:
        payload = parts[1] + "=" * (-len(parts[1]) % 4)
        claims = json.loads(base64.urlsafe_b64decode(payload))
    except (ValueError, UnicodeDecodeError, json.JSONDecodeError, binascii.Error):
        claims = {}
    if str(claims.get("role", "")).lower() == "service_role":
        sys.exit("Refusing to package a legacy Supabase service-role key")
PY
}

validate_config_file() {
  local config_key
  config_key="$(python3 - "$1" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as source:
    document = json.load(source)
key = document.get("publishable_key")
if not isinstance(key, str):
    sys.exit("Supabase configuration has no string publishable_key")
print(key)
PY
)"
  validate_public_key "$config_key"
}

if [[ -n "${CONFIG_PATH:-}" ]]; then
  validate_config_file "$CONFIG_PATH"
elif [[ -n "${SUPABASE_URL:-}" || -n "${SUPABASE_PUBLISHABLE_KEY:-}" ]]; then
  if [[ -z "${SUPABASE_URL:-}" || -z "${SUPABASE_PUBLISHABLE_KEY:-}" ]]; then
    echo "SUPABASE_URL and SUPABASE_PUBLISHABLE_KEY must be supplied together" >&2
    exit 1
  fi
  validate_public_key "$SUPABASE_PUBLISHABLE_KEY"
elif [[ -f "$repository/stellarion-config.json" ]]; then
  validate_config_file "$repository/stellarion-config.json"
else
  validate_config_file "$repository/stellarion-config.example.json"
fi

case "$output_root" in
  "$repository"/dist|"$repository"/dist/*) ;;
  *) echo "Refusing to clean output outside $repository/dist" >&2; exit 1 ;;
esac

mkdir -p "$output_root"
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
if [[ -n "${CONFIG_PATH:-}" ]]; then
  cp "$CONFIG_PATH" "$stage/stellarion-config.json"
elif [[ -n "${SUPABASE_URL:-}" && -n "${SUPABASE_PUBLISHABLE_KEY:-}" ]]; then
  STELLARION_NATIVE_STAGE="$stage" python3 -c 'import json, os, pathlib; pathlib.Path(os.environ["STELLARION_NATIVE_STAGE"], "stellarion-config.json").write_text(json.dumps({"url": os.environ["SUPABASE_URL"], "publishable_key": os.environ["SUPABASE_PUBLISHABLE_KEY"]}, indent=2) + "\n", encoding="utf-8")'
elif [[ -f "$repository/stellarion-config.json" ]]; then
  cp "$repository/stellarion-config.json" "$stage/"
else
  cp "$repository/stellarion-config.example.json" "$stage/"
fi

rm -f -- "$archive"
(cd "$stage" && zip -qr "$archive" .)
echo "Created $archive"
