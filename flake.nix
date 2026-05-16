{
  description = "Wondering";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/master";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    psx-tests = {
      url = "github:PeterLemon/PSX";
      flake = false;
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
      psx-tests,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;

          overlays = [ rust-overlay.overlays.default ];
        };

        rustToolchain = pkgs.rust-bin.nightly.latest.default;
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };

        test-rom-runner = rustPlatform.buildRustPackage {
          name = "test-rom-runner";
          version = "6.6.6";

          src = ./.;
          buildAndTestSubdir = "rom-tests";

          cargoLock = {
            lockFile = ./Cargo.lock;
            allowBuiltinFetchGit = true;
          };
        };

        bios = pkgs.fetchurl {
          url = "https://github.com/Abdess/retrobios/raw/refs/heads/main/bios/Sony/PlayStation/scph1001.bin";
          hash = "sha256-ca+U0eR6aMEej9ufg2gEBgFRSkKlo5nNpIx9O/8emdM=";
        };
      in
      {
        formatter = pkgs.nixpkgs-fmt;

        packages = {
          bios = pkgs.callPackage ./rom-tests/bios.nix {
            inherit bios test-rom-runner;
          };
          cpu-tests = pkgs.callPackage ./rom-tests/test-rom.nix {
            inherit test-rom-runner;
            test-dir = "${psx-tests}/CPUTest/CPU/";
          };
          gpu-tests = pkgs.callPackage ./rom-tests/test-rom.nix {
            inherit test-rom-runner;
            test-dir = "${psx-tests}/GPU/";
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [ rustToolchain ];
        };
      }
    );
}
