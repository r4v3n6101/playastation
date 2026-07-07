{ inputs, ... }: {
  perSystem =
    {
      pkgs,
      lib,
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

      craneCommonArgs = {
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
      _module.args = {
        inherit craneLib craneCommonArgs;

        pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [
            inputs.rust-overlay.overlays.default
          ];
        };
      };
    };
}
