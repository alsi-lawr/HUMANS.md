#!/usr/bin/env bash
set -euo pipefail

case "${1:-}" in
  --check)
    prettier_mode=--check
    rust_mode=(-- --check)
    ;;
  --write)
    prettier_mode=--write
    rust_mode=()
    ;;
  *)
    echo "usage: scripts/format-source.sh --check|--write" >&2
    exit 2
    ;;
esac

root="$(git rev-parse --show-toplevel)"
cd "$root"

mapfile -d '' candidates < <(
  git ls-files --cached --others --exclude-standard -z -- '*.md' '*.ts' '*.tsx'
)
prettier_files=()
for candidate in "${candidates[@]}"; do
  if [[ -f "$candidate" ]]; then
    prettier_files+=("$root/$candidate")
  fi
done
(
  cd casefile/web
  bun run prettier "$prettier_mode" "${prettier_files[@]}"
)

(
  cd casefile
  cargo fmt --all "${rust_mode[@]}"
)
