#!/usr/bin/env bash
# Build the documentation-only Store and populate it through Casefile's sole progress writer.
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
transition="$source_root/casefile/casefile-workflow/scripts/transition-ticket-progress.py"

[[ -x "$casefile_bin" ]] || { echo "casefile binary is not executable: $casefile_bin" >&2; exit 2; }
[[ -f "$transition" ]] || { echo "released progress script is missing: $transition" >&2; exit 2; }
[[ "$scratch" != /tmp && "$scratch" != /tmp/* ]] || { echo "scratch must not be under /tmp" >&2; exit 2; }
[[ "$store" != "$scratch" && "$store" == "$scratch"/* ]] || {
  echo "store must be a strict contained child of scratch" >&2
  exit 2
}
rm -rf "$store"
mkdir -p "$scratch/previews"
cp -R "$template" "$store"

investigation="projects/demo/investigations/sample"
preview="$scratch/previews/bootstrap.json"
python "$transition" --root "$store" --casefile "$casefile_bin" --preview-file "$preview" \
  bootstrap-unknown --investigation "$investigation" >/dev/null
python "$transition" --root "$store" --casefile "$casefile_bin" --preview-file "$preview" --apply \
  bootstrap-unknown --investigation "$investigation" >/dev/null

transition_ticket() {
  local ticket=$1 to=$2 stamp=$3 id=$4
  preview="$scratch/previews/$id.json"
  python "$transition" --root "$store" --casefile "$casefile_bin" --preview-file "$preview" \
    transition --investigation "$investigation" --recorded-by "documentation-demo" \
    --recorded-at "$stamp" --ticket "$ticket" --from unknown --to "$to" --operation-id "$id" >/dev/null
  python "$transition" --root "$store" --casefile "$casefile_bin" --preview-file "$preview" --apply \
    transition --investigation "$investigation" --recorded-by "documentation-demo" \
    --recorded-at "$stamp" --ticket "$ticket" --from unknown --to "$to" --operation-id "$id" >/dev/null
}

transition_ticket HMD-011 in_progress "2026-07-26T18:50:00Z" demo-011-start
transition_ticket HMD-012 in_review "2026-07-26T18:51:00Z" demo-012-review
transition_ticket HMD-013 verifying "2026-07-26T18:52:00Z" demo-013-verify
transition_ticket HMD-014 blocked "2026-07-26T18:53:00Z" demo-014-blocked
transition_ticket HMD-015 complete "2026-07-26T18:54:00Z" demo-015-complete

"$casefile_bin" --root "$store" check >/dev/null
