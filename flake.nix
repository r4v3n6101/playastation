{
  description = "Wondering";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/master";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";

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
      crane,
      psx-tests,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;

          overlays = [ rust-overlay.overlays.default ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain (p: p.rust-bin.nightly.latest.default);

        test-rom-runner = craneLib.buildPackage rec {
          inherit (craneLib.crateNameFromCargoToml { cargoToml = ./rom-tests/Cargo.toml; }) pname;
          inherit (craneLib.crateNameFromCargoToml { cargoToml = ./Cargo.toml; }) version;

          src = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              (craneLib.fileset.commonCargoSources ./core)
              (craneLib.fileset.commonCargoSources ./rom-tests)
            ];
          };
          strictDeps = true;

          cargoArtifacts = craneLib.buildDepsOnly {
            inherit (craneLib.crateNameFromCargoToml { cargoToml = ./core/Cargo.toml; }) pname;
            inherit (craneLib.crateNameFromCargoToml { cargoToml = ./Cargo.toml; }) version;

            inherit src strictDeps;
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
          buildInputs = [ pkgs.rust-bin.nightly.latest.default ];
        };
      }
    );
}
