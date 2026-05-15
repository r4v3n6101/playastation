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
      in
      {
        formatter = pkgs.nixpkgs-fmt;

        packages = {
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
          cpu-tests = pkgs.callPackage ./rom-tests/cpu.nix {
            inherit psx-tests;
            runner = self.packages.${system}.test-rom-runner;
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [ rustToolchain ];

          PSX_BIOS = pkgs.fetchurl {
            url = "https://github.com/Abdess/retrobios/raw/refs/heads/main/bios/Sony/PlayStation/scph1001.bin";
            hash = "sha256-ca+U0eR6aMEej9ufg2gEBgFRSkKlo5nNpIx9O/8emdM=";
          };
        };
      }
    );
}
