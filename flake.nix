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
          config.allowUnfreePredicate = package:
            nixpkgs.lib.getName package == "c2000-cgt";
        };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = ["rust-src" "rustfmt" "clippy" "rust-analyzer"];
        };

        cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
        neoCargoToml = builtins.fromTOML (builtins.readFile ./crates/mint-neo/Cargo.toml);

        mintPkg = pkgs.rustPlatform.buildRustPackage {
          pname = "mint";
          version = cargoToml.workspace.package.version;
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = ["-p" "mint-cli"];
          cargoTestFlags = ["-p" "mint-cli"];
          buildType = "release";
        };
        mintNeoProbePkg = pkgs.rustPlatform.buildRustPackage {
          pname = "mint-neo-probe";
          version = neoCargoToml.package.version;
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = ["-p" "mint-neo" "--bin" "mint-neo"];
          doCheck = false;
          buildType = "release";
        };

        generateAbiHeaders = abi: extraExampleReplaces: ''
          substitute ${./doc/examples/block.toml} layout.toml \
            --replace-fail 'abi = "generic-le"' 'abi = "${abi}"' ${extraExampleReplaces}
          substitute ${./tests/abi/pack.toml} pack.toml \
            --replace-fail 'abi = "generic-le"' 'abi = "${abi}"'
          ${mintPkg}/bin/mint header layout.toml -o mint_abi.h
          ${mintPkg}/bin/mint header pack.toml -o mint_pack.h
        '';
        generateNeoProbe = abi: ''
          substitute ${./tests/abi/neo-schema.h} mint_neo.h \
            --replace-fail '@mint abi generic-le' '@mint abi ${abi}'
          ${mintNeoProbePkg}/bin/mint-neo inspect mint_neo.h --format json > mint_neo_layout.json
          ${pkgs.jq}/bin/jq -r -f ${./tests/abi/neo-expect.jq} \
            mint_neo_layout.json > mint_neo_expect.h
        '';
        mkGccAbiProbe = {abi, compiler, flags}:
          pkgs.runCommand "mint-abi-${abi}" {} ''
            ${generateAbiHeaders abi ""}
            ${generateNeoProbe abi}
            ${compiler} ${nixpkgs.lib.escapeShellArgs flags} \
              -I. -c ${./tests/abi/compiler-probe.c} -o probe.o
            ${compiler} ${nixpkgs.lib.escapeShellArgs flags} \
              -I. -c ${./tests/abi/neo-compiler-probe.c} -o neo-probe.o
            touch $out
          '';
      in {
        packages = {
          default = mintPkg;
          mint = mintPkg;
        };

        checks = nixpkgs.lib.optionalAttrs (system == "x86_64-linux") (let
          armGcc = pkgs.pkgsCross.arm-embedded.buildPackages.gccWithoutTargetLibc;
          riscvGcc = pkgs.pkgsCross.riscv32-embedded.buildPackages.gccWithoutTargetLibc;
          commonFlags = ["-std=c11" "-ffreestanding" "-Wall" "-Wextra" "-Werror" "-pedantic"];
          armFlags = ["-mcpu=cortex-m3" "-mthumb" "-mabi=aapcs" "-mfloat-abi=soft" "-DMINT_ARM"];
          armProbe = abi: flags: mkGccAbiProbe {
            inherit abi;
            compiler = "${armGcc}/bin/arm-none-eabi-gcc";
            flags = commonFlags ++ armFlags ++ flags;
          };
        in {
          abi-generic-le = armProbe "generic-le" [];
          abi-arm-aapcs32-le = armProbe "arm-aapcs32-le" [];
          abi-riscv-ilp32-le = mkGccAbiProbe {
            abi = "riscv-ilp32-le";
            compiler = "${riscvGcc}/bin/riscv32-none-elf-gcc";
            flags = commonFlags ++ ["-march=rv32imac" "-mabi=ilp32" "-DMINT_RISCV"];
          };
          abi-generic-be = armProbe "generic-be" ["-mbig-endian" "-DMINT_EXPECT_BIG_ENDIAN"];
          abi-ti-c28x-eabi = pkgs.runCommand "mint-abi-ti-c28x-eabi" {} ''
            ${generateAbiHeaders "ti-c28x-eabi" "--replace-fail 'type = \"u8\"' 'type = \"u16\"'"}
            ${generateNeoProbe "ti-c28x-eabi"}
            ${pkgs.c2000-cgt}/bin/cl2000 --abi=eabi --c11 --compile_only --quiet \
              --define=MINT_TI_C28X --include_path=${pkgs.c2000-cgt}/include --include_path=. --output_file=probe.obj \
              ${./tests/abi/compiler-probe.c}
            ${pkgs.c2000-cgt}/bin/cl2000 --abi=eabi --c11 --compile_only --quiet \
              --include_path=${pkgs.c2000-cgt}/include --include_path=. --output_file=neo-probe.obj \
              ${./tests/abi/neo-compiler-probe.c}
            touch $out
          '';
        });

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rustToolchain
            uv
          ];
        };
      }
    );
}
