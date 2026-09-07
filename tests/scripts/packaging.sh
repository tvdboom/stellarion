#!/usr/bin/env bash
# Run with: bash tests/scripts/packaging.sh
set -euo pipefail
source_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
source "$source_root/scripts/common.sh"
for channel in '../../outside' 'channel/name'; do
  if CHANNEL="$channel" TARGET=test SKIP_BUILD=1 bash "$source_root/scripts/package-native.sh" >/dev/null 2>&1; then
    echo "Accepted unsafe package channel: $channel" >&2
    exit 1
  fi
done
mkdir -p "$source_root/target"
fixture="$(mktemp -d "$source_root/target/packaging-verification-XXXXXX")"
[[ "$fixture" == "$source_root"/target/packaging-verification-* && -d "$fixture" ]]
trap 'rm -rf -- "$fixture"' EXIT
repository="$fixture/repository"
mkdir -p "$repository/dist" "$fixture/outside"
OUTPUT_DIRECTORY="dist/nested"
prepare_package_output
[[ "$output_root" == "$repository/dist/nested" ]]
for OUTPUT_DIRECTORY in "$repository/dist/../../outside" "$repository/dist-neighbor"; do
  if (prepare_package_output) 2>/dev/null; then
    echo "Accepted unsafe package path: $OUTPUT_DIRECTORY" >&2
    exit 1
  fi
done
# MSYS requires explicit native-link support; exercise symlinks wherever the shell supports them.
if ln -s "$fixture/outside" "$repository/dist/linked" && [[ -L "$repository/dist/linked" ]]; then
  OUTPUT_DIRECTORY="$repository/dist/linked/package"
  if (prepare_package_output) 2>/dev/null; then
    echo "Accepted a package destination through a symlink" >&2
    exit 1
  fi
fi
echo 'Bash packaging path checks passed.'
