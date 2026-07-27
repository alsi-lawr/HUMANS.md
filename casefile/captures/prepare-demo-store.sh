#!/usr/bin/env bash
# Build the documentation-only Store from its reviewed static fixture.
set -euo pipefail

if [[ $# -ne 4 ]]; then
  echo "usage: $0 <source-root> <casefile-bin> <scratch-dir> <store-dir>" >&2
  exit 2
fi
source_root=$(realpath "$1")
casefile_bin=$(realpath "$2")
scratch=$(realpath -e "$3")
store=$(realpath -m "$4")
script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
template="$script_dir/fixture/demo-store"

[[ -x "$casefile_bin" ]] || { echo "casefile binary is not executable: $casefile_bin" >&2; exit 2; }
[[ -f "$source_root/casefile/Cargo.toml" ]] || { echo "released Casefile source is missing" >&2; exit 2; }
[[ "$scratch" != /tmp && "$scratch" != /tmp/* ]] || { echo "scratch must not be under /tmp" >&2; exit 2; }
[[ "$store" != "$scratch" && "$store" == "$scratch"/* ]] || {
  echo "store must be a strict contained child of scratch" >&2
  exit 2
}
rm -rf "$store"
mkdir -p "$scratch"
cp -R "$template" "$store"

"$casefile_bin" --root "$store" check >/dev/null
