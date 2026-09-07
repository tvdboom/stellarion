#!/usr/bin/env bash

# Resolve package output below dist and reject parent traversal or linked ancestors before cleanup.
prepare_package_output() {
  output_root="${OUTPUT_DIRECTORY:-$repository/dist}"
  [[ "$output_root" == /* ]] || output_root="$repository/$output_root"
  output_root="${output_root%/}"
  case "$output_root" in
    "$repository"/dist|"$repository"/dist/*) ;;
    *) echo "Refusing to clean output outside $repository/dist" >&2; return 1 ;;
  esac
  case "/$output_root/" in
    */../*|*/./*) echo "Package output must not contain parent or current directory components" >&2; return 1 ;;
  esac
  local ancestor="$output_root"
  while [[ "$ancestor" != "$repository" && "$ancestor" != / ]]; do
    if [[ -L "$ancestor" ]]; then
      echo "Refusing to package through linked path $ancestor" >&2
      return 1
    fi
    ancestor="$(dirname "$ancestor")"
  done
  mkdir -p "$output_root"
  output_root="$(cd "$output_root" && pwd -P)"
}
