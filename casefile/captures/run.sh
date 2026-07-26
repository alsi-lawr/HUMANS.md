#!/usr/bin/env bash
# Recreate the committed documentation media from the published v0.3.4 source.
set -euo pipefail

fail() {
  echo "$1" >&2
  exit 2
}

require_clean_checkout() {
  [[ -z "$(git -C "$1" status --porcelain --untracked-files=all)" ]] || fail "checkout has uncommitted or untracked changes: $1"
}

require_session_scratch() {
  local source_root=$1 scratch=$2 workspace_input workspace session
  workspace_input="$source_root/.agent-workspace"
  [[ ! -L "$workspace_input" ]] || fail "workspace root must not be a symlink: $workspace_input"
  mkdir -p "$workspace_input"
  [[ -d "$workspace_input" && ! -L "$workspace_input" ]] || fail "workspace root is not a directory: $workspace_input"
  workspace=$(realpath -e "$workspace_input")
  [[ "$workspace" == "$source_root/.agent-workspace" ]] || fail "workspace root escapes source: $workspace_input"
  scratch=$(realpath -m "$scratch")
  [[ "$scratch" == "$workspace"/* ]] || fail "scratch must be under $workspace/<session>"
  session=${scratch#"$workspace/"}
  [[ -n "$session" && "$session" != */* ]] || fail "scratch must be one session directory under $workspace"
}

if [[ $# -eq 2 && "$1" == "--check-clean-checkout" ]]; then
  require_clean_checkout "$2"
  exit 0
fi

if [[ $# -eq 3 && "$1" == "--check-scratch" ]]; then
  require_session_scratch "$2" "$3"
  mkdir -p "$(realpath -m "$3")"
  exit 0
fi

if [[ $# -ne 5 ]]; then
  echo "usage: $0 <v0.3.4-source> <wiki-checkout> <viset-checkout> <vhs-checkout> <scratch-dir>" >&2
  exit 2
fi
source_root=$(realpath "$1")
wiki_root=$(realpath "$2")
viset_root=$(realpath "$3")
vhs_root=$(realpath "$4")
scratch=$(realpath -m "$5")
script_dir=$(cd -- "$(dirname -- "$0")" && pwd)

require_session_scratch "$source_root" "$scratch"
[[ $(git -C "$source_root" rev-parse 'v0.3.4^{}') == 7cd49f04aacc34f3f7b27d60aa0c2ee3f771c5e7 ]] || fail "v0.3.4 tag is not the published source commit"
[[ $(git -C "$source_root" rev-parse 'v0.3.4^{tree}') == 619e71f0cb34839d3d2e8898fc193e25e53ef18a ]] || fail "v0.3.4 tree does not match the published release"
[[ $(git -C "$source_root" rev-parse HEAD) == 7cd49f04aacc34f3f7b27d60aa0c2ee3f771c5e7 ]] || fail "source checkout is not the published v0.3.4 commit"
[[ $(git -C "$viset_root" rev-parse HEAD) == 370ef7b656378487486a498589cac6419cfcd861 ]] || fail "Viset revision mismatch"
[[ $(git -C "$vhs_root" rev-parse HEAD) == bb4e27a982f4f126b3c71bbab8cbb08bad02002a ]] || fail "VHS revision mismatch"
for checkout in "$source_root" "$wiki_root" "$viset_root" "$vhs_root"; do
  require_clean_checkout "$checkout"
done

mkdir -p "$scratch" "$wiki_root/assets/casefile"
# Build the CLI from the checked release source. The only local build reference is task scratch.
nix build "$source_root#casefile" --out-link "$scratch/casefile-release"
casefile_bin="$scratch/casefile-release/bin/casefile"
GOCACHE="$scratch/go-cache" GOPATH="$scratch/go" \
  nix shell nixpkgs#go --command bash -c "cd \"$vhs_root\" && go build -o \"$scratch/vhs\" ."
store="$scratch/demo-store"
"$script_dir/prepare-demo-store.sh" "$source_root" "$casefile_bin" "$scratch" "$store"

mkdir -p "$scratch/browser" "$scratch/tui"
CASEFILE_BIN="$casefile_bin" CASEFILE_ROOT="$store" CASEFILE_INDEX="$scratch/browser/casefile.sqlite" \
  nix run "$viset_root" -- capture "$script_dir/browser-board.lua" --output "$scratch/browser"

cat > "$scratch/tui/launch-casefile.sh" <<EOF
#!/usr/bin/env bash
exec "$casefile_bin" --root "$store" tui
EOF
chmod 700 "$scratch/tui/launch-casefile.sh"
(
  cd "$scratch/tui"
  nix shell nixpkgs#ttyd nixpkgs#chromium --command "$scratch/vhs" --capture-mode=record "$script_dir/terminal-workbench.tape"
)

install -m 0644 "$scratch/browser/browser-workbench.png" "$wiki_root/assets/casefile/browser-workbench.png"
install -m 0644 "$scratch/tui/terminal-workbench.png" "$wiki_root/assets/casefile/terminal-workbench.png"
install -m 0644 "$scratch/tui/terminal-workbench.webp" "$wiki_root/assets/casefile/terminal-workbench.webp"
printf 'Created media from v0.3.4 (%s).\n' "$(git -C "$source_root" rev-parse HEAD)"
