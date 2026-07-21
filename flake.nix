{
  description = "humans-md development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    { nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            act
            actionlint
            bun
            cargo
            clippy
            gh
            git
            nodejs_24
            prettier
            python314
            rustc
            rustfmt
          ] ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
            docker-client
            util-linux
          ];

          shellHook = ''
            echo "humans-md development shell"
            echo "  Setup:   (cd casefile/web && bun install)"
            echo "  Format: scripts/format-source.sh --check|--write"
            echo "  Web:    cd casefile/web; bun install; bun run typecheck; bun run test; bun run build"
            echo "  Rust:   cd casefile; cargo fmt --check; cargo clippy --workspace --all-targets -- -D warnings; cargo test --workspace"
            echo "  CI:     act pull_request -j validate --pull=false -P ubuntu-latest=catthehacker/ubuntu:act-latest"
          '';
        };
      }
    );
}
