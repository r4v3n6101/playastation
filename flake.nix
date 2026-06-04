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

    bios = {
      url = "https://github.com/Abdess/retrobios/raw/refs/heads/main/bios/Sony/PlayStation/scph1001.bin";
      flake = false;
    };

    peter-lemon-test-roms = {
      url = "github:PeterLemon/PSX";
      flake = false;
    };

    amidog-cpu-test-rom = {
      url = "tarball+https://psx.amidog.se/lib/exe/fetch.php?media=psx:download:psxtest_cpu.zip";
      flake = false;
    };

    amidog-cpx-test-rom = {
      url = "tarball+https://psx.amidog.se/lib/exe/fetch.php?media=psx:download:psxtest_cpx.zip";
      flake = false;
    };

    pcsx-redux = {
      url = "git+https://github.com/nicolasnoble/pcsx-redux?submodules=1";
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

      bios,
      peter-lemon-test-roms,
      amidog-cpu-test-rom,
      amidog-cpx-test-rom,
      pcsx-redux,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;

          overlays = [ rust-overlay.overlays.default ];
        };

        inherit (pkgs) lib;
        craneLib = (crane.mkLib pkgs).overrideToolchain (p: p.rust-bin.nightly.latest.default);

        winit-libs = lib.makeLibraryPath (
          with pkgs;
          [
            wayland
            wayland-protocols

            libxkbcommon
            libx11
            libxcursor
            libxi
            libxrandr
            libxext
          ]
        );

        test-rom-runner = craneLib.buildPackage rec {
          inherit (craneLib.crateNameFromCargoToml { cargoToml = ./rom-tests/Cargo.toml; }) pname;
          inherit (craneLib.crateNameFromCargoToml { cargoToml = ./Cargo.toml; }) version;

          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              (craneLib.fileset.commonCargoSources ./core)
              (craneLib.fileset.commonCargoSources ./rom-tests)
            ];
          };
          strictDeps = true;

          nativeBuildInputs = with pkgs; [
            makeWrapper
          ];

          postInstall = lib.optionalString pkgs.stdenv.isLinux ''
            wrapProgram $out/bin/test-rom-runner \
              --prefix LD_LIBRARY_PATH : ${winit-libs}
          '';

          cargoArtifacts = craneLib.buildDepsOnly {
            inherit (craneLib.crateNameFromCargoToml { cargoToml = ./core/Cargo.toml; }) pname;
            inherit (craneLib.crateNameFromCargoToml { cargoToml = ./Cargo.toml; }) version;

            inherit src strictDeps;
          };
        };
      in
      {
        formatter = pkgs.nixpkgs-fmt;

        packages = {
          peter-lemon-tests = pkgs.lib.makeScope pkgs.newScope (self: {
            cpu = pkgs.callPackage ./rom-tests/peter-lemon.nix {
              inherit test-rom-runner bios;
              test-dir = "${peter-lemon-test-roms}/CPUTest/CPU/";
            };
            gpu = pkgs.callPackage ./rom-tests/peter-lemon.nix {
              inherit test-rom-runner bios;
              test-dir = "${peter-lemon-test-roms}/GPU/";
            };
            cube = pkgs.callPackage ./rom-tests/peter-lemon.nix {
              inherit test-rom-runner bios;
              test-dir = "${peter-lemon-test-roms}/Cube/";
            };
          });

          amidog-tests = pkgs.lib.makeScope pkgs.newScope (self: {
            cpu = pkgs.callPackage ./rom-tests/amidog.nix {
              inherit test-rom-runner bios;
              test-rom = "${amidog-cpu-test-rom}/psxtest_cpu.exe";
            };
            cpx = pkgs.callPackage ./rom-tests/amidog.nix {
              inherit test-rom-runner bios;
              test-rom = "${amidog-cpx-test-rom}/psxtest_cpx.exe";
            };
          });

          pcsx-redux-tests = pkgs.lib.makeScope pkgs.newScope (self: {
            basic = pkgs.callPackage ./rom-tests/pcsx-redux-tests.nix {
              inherit test-rom-runner bios pcsx-redux;
              kind = "basic";
            };
            cop0 = pkgs.callPackage ./rom-tests/pcsx-redux-tests.nix {
              inherit test-rom-runner bios pcsx-redux;
              kind = "cop0";
            };
            cpu = pkgs.callPackage ./rom-tests/pcsx-redux-tests.nix {
              inherit test-rom-runner bios pcsx-redux;
              kind = "cpu";
            };
            gpu = pkgs.callPackage ./rom-tests/pcsx-redux-tests.nix {
              inherit test-rom-runner bios pcsx-redux;
              kind = "gpu";
            };
          });
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rust-bin.nightly.latest.default
            cargo-show-asm
            samply
          ];

          env = lib.optionalAttrs pkgs.stdenv.isLinux {
            LD_LIBRARY_PATH = winit-libs;
          };
        };
      }
    );
}
