#!/usr/bin/env bash
# Exercise the runner's refusal paths using only a disposable session scratch directory.
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 <humans-md-source> <session-scratch>" >&2
  exit 2
fi
source_root=$(realpath "$1")
scratch=$(realpath -e "$2")
script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
workspace=$(realpath -e "$source_root/.agent-workspace")

[[ "$scratch" == "$workspace"/* ]] || { echo "scratch must be under source .agent-workspace" >&2; exit 2; }
test_root="$scratch/guard-checks"
case "$test_root" in "$scratch"/*) ;; *) echo "unsafe test path" >&2; exit 2;; esac
rm -rf "$test_root"
mkdir -p "$test_root/untracked-input"
git -C "$test_root/untracked-input" init -q
git -C "$test_root/untracked-input" config user.name capture-test
git -C "$test_root/untracked-input" config user.email capture-test@example.invalid
printf 'tracked\n' > "$test_root/untracked-input/tracked"
git -C "$test_root/untracked-input" add tracked
git -C "$test_root/untracked-input" commit -qm initial
printf 'untracked\n' > "$test_root/untracked-input/untracked"

fresh_source="$test_root/fresh-source"
mkdir -p "$fresh_source"
git -C "$fresh_source" init -q
git -C "$fresh_source" config user.name capture-test
git -C "$fresh_source" config user.email capture-test@example.invalid
printf '.agent-workspace/\n' > "$fresh_source/.gitignore"
printf 'tracked\n' > "$fresh_source/tracked"
git -C "$fresh_source" add .gitignore tracked
git -C "$fresh_source" commit -qm initial
fresh_session="$fresh_source/.agent-workspace/session"
"$script_dir/run.sh" --check-scratch "$fresh_source" "$fresh_session"
[[ -d "$fresh_source/.agent-workspace" && ! -L "$fresh_source/.agent-workspace" && -d "$fresh_session" ]] || {
  echo "absent workspace root or session scratch was not created" >&2
  exit 1
}
"$script_dir/run.sh" --check-clean-checkout "$fresh_source"

if "$script_dir/run.sh" --check-clean-checkout "$test_root/untracked-input" >/dev/null 2>&1; then
  echo "untracked checkout was not rejected" >&2
  exit 1
fi
if "$script_dir/run.sh" --check-scratch "$source_root" "$scratch/nested" >/dev/null 2>&1; then
  echo "nested scratch was not rejected" >&2
  exit 1
fi
mkdir -p "$test_root/escaped"
ln -s "$test_root/escaped" "$fresh_source/.agent-workspace/escape"
if "$script_dir/run.sh" --check-scratch "$fresh_source" "$fresh_source/.agent-workspace/escape" >/dev/null 2>&1; then
  echo "symlink-escaping scratch was not rejected" >&2
  exit 1
fi
if "$script_dir/prepare-demo-store.sh" "$source_root" /usr/bin/true "$scratch" "$workspace/other-session" >/dev/null 2>&1; then
  echo "out-of-scratch store was not rejected" >&2
  exit 1
fi
if "$script_dir/prepare-demo-store.sh" "$source_root" /usr/bin/true "$scratch" "$scratch" >/dev/null 2>&1; then
  echo "scratch itself was accepted as a store" >&2
  exit 1
fi
ln -s "$workspace/other-session" "$test_root/escape"
if "$script_dir/prepare-demo-store.sh" "$source_root" /usr/bin/true "$scratch" "$test_root/escape" >/dev/null 2>&1; then
  echo "symlink-escaping store was not rejected" >&2
  exit 1
fi
rm -rf "$test_root"
printf 'capture guard refusal checks passed\n'
