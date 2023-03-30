{
  description = "Nix development environment for Ziggurat";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };

        localCiScript = pkgs.writeScriptBin "ci-local" ''
          echo "Running cargo check..."
          cargo check --all-targets

          echo "Running cargo fmt check..."
          cargo +nightly fmt --all -- --check

          echo "Running cargo clippy..."
          cargo clippy --all-targets -- -D warnings

          echo "Running cargo-sort check..."
          cargo-sort -cw
        '';
      in
      {
        devShells = {
          default = pkgs.mkShell {
            buildInputs = [ localCiScript ] ++ (with pkgs; [
              rustup
              cargo-sort
            ]);

            shellHook = ''
              rustup default stable
              rustup toolchain install nightly --allow-downgrade --profile minimal --component rustfmt
            '';
          };
        };
      });
}
