{ ... }: {
  perSystem =
    {
      pkgs,
      lib,
      craneLib,
      craneCommonArgs,
      ...
    }:
    {
      packages.frontend-desktop = craneLib.buildPackage (
        craneCommonArgs
        // rec {
          inherit (craneLib.crateNameFromCargoToml { cargoToml = ./Cargo.toml; }) pname;
          inherit (craneLib.crateNameFromCargoToml { cargoToml = ../Cargo.toml; }) version;

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
            craneCommonArgs
            // {
              inherit pname version cargoExtraArgs;
            }
          );
        }
      );
    };
}
