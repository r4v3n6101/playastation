{ inputs, ... }: {
  perSystem =
    {
      pkgs,
      lib,
      self',
      system,
      ...
    }:
    let
      craneLib = (inputs.crane.mkLib pkgs).overrideToolchain (
        p:
        p.rust-bin.nightly.latest.default.override {
          targets = [ "wasm32-unknown-unknown" ];
        }
      );

      commonArgs = {
        src = lib.fileset.toSource rec {
          root = ./.;
          fileset = lib.fileset.unions [
            # Rust-specific (locks, toml-s, .rs)
            (craneLib.fileset.commonCargoSources ./.)

            # Web specific like assets, html-s
            (lib.fileset.fileFilter (
              file:
              lib.any file.hasExt [
                "html"
              ]
            ) root)
          ];
        };

        strictDeps = true;
      };

    in
    {
      _module.args.pkgs = import inputs.nixpkgs {
        inherit system;

        overlays = [
          inputs.rust-overlay.overlays.default
        ];
      };

      packages = {
        frontend-desktop = craneLib.buildPackage (
          commonArgs
          // rec {
            inherit (craneLib.crateNameFromCargoToml { cargoToml = ./frontend-desktop/Cargo.toml; }) pname;
            inherit (craneLib.crateNameFromCargoToml { cargoToml = ./Cargo.toml; }) version;

            cargoExtraArgs = "-p ${pname}";

            nativeBuildInputs = with pkgs; [ makeWrapper ];

            postInstall = lib.optionalString pkgs.stdenv.isLinux ''
              wrapProgram $out/bin/playastation-desktop \
                --prefix LD_LIBRARY_PATH : ${
                  lib.makeLibraryPath (
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
                  )
                }
            '';

            cargoArtifacts = craneLib.buildDepsOnly (
              commonArgs
              // {
                inherit pname version cargoExtraArgs;
              }
            );
          }
        );

        frontend-web = craneLib.buildTrunkPackage (
          commonArgs
          // rec {
            inherit (craneLib.crateNameFromCargoToml { cargoToml = ./frontend-web/Cargo.toml; }) pname;
            inherit (craneLib.crateNameFromCargoToml { cargoToml = ./Cargo.toml; }) version;

            trunkExtraArgs = "--public-url /playastation/";
            cargoExtraArgs = "-p ${pname} --target wasm32-unknown-unknown";
            CARGO_BUILD_TARGET = "wasm32-unknown-unknown";

            wasm-bindgen-cli = pkgs.wasm-bindgen-cli_0_2_121;

            preBuild = ''
              cd frontend-web/
            '';

            postBuild = ''
              mv dist ..
              cd ..
            '';

            cargoArtifacts = craneLib.buildDepsOnly (
              commonArgs
              // {
                inherit
                  pname
                  version
                  cargoExtraArgs
                  CARGO_BUILD_TARGET
                  ;
              }
            );
          }
        );
        wasm-serve = pkgs.writeShellScriptBin "wasm-serve" ''
          ${pkgs.python3Minimal}/bin/python3 -m http.server --directory ${self'.packages.frontend-web} 8000
        '';
      };

      devShells.default = craneLib.devShell {
        packages = with pkgs; [
          cargo-show-asm
          samply
          wasm-bindgen-cli
          trunk
        ];
      };
    };
}
