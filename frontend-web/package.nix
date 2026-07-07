{ ... }: {
  perSystem =
    {
      pkgs,
      lib,
      self',
      craneLib,
      craneCommonArgs,
      ...
    }:
    {
      packages = {
        frontend-web = craneLib.buildTrunkPackage (
          craneCommonArgs
          // rec {
            inherit (craneLib.crateNameFromCargoToml { cargoToml = ./Cargo.toml; }) pname;
            inherit (craneLib.crateNameFromCargoToml { cargoToml = ../Cargo.toml; }) version;

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
              craneCommonArgs
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
    };
}
