{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    rust-overlay,
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [(import rust-overlay)];
        };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = ["rust-src" "rustfmt" "clippy" "rust-analyzer"];
        };

        cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);

        mintPkg = pkgs.rustPlatform.buildRustPackage {
          pname = "mint";
          version = cargoToml.workspace.package.version;
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = ["-p" "mint-cli"];
          cargoTestFlags = ["-p" "mint-cli"];
          buildType = "release";
        };
      in {
        packages = {
          default = mintPkg;
          mint = mintPkg;
        };

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rustToolchain
            uv
          ];
        };
      }
    );
}
