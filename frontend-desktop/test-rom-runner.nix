{ inputs, ... }: {
  perSystem =
    { pkgs, lib, ... }:
    let
      craneLib = (inputs.crane.mkLib pkgs).overrideToolchain (p: p.rust-bin.nightly.latest.default);
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
    in
    {
      packages.test-rom-runner = craneLib.buildPackage rec {
        inherit (craneLib.crateNameFromCargoToml { cargoToml = ./Cargo.toml; }) pname;
        inherit (craneLib.crateNameFromCargoToml { cargoToml = ../Cargo.toml; }) version;

        src = lib.fileset.toSource {
          root = ../.;
          fileset = lib.fileset.unions [
            ../Cargo.toml
            ../Cargo.lock
            (craneLib.fileset.commonCargoSources ../core)
            (craneLib.fileset.commonCargoSources ../frontend-common)
            (craneLib.fileset.commonCargoSources ../frontend-desktop)
          ];
        };
        strictDeps = true;

        nativeBuildInputs = with pkgs; [
          makeWrapper
        ];

        postInstall = ''
          mv $out/bin/playastation-frontend-desktop $out/bin/test-rom-runner
        ''
        + lib.optionalString pkgs.stdenv.isLinux ''
          wrapProgram $out/bin/test-rom-runner \
            --prefix LD_LIBRARY_PATH : ${winit-libs}
        '';

        cargoArtifacts = craneLib.buildDepsOnly {
          inherit (craneLib.crateNameFromCargoToml { cargoToml = ../core/Cargo.toml; }) pname;
          inherit (craneLib.crateNameFromCargoToml { cargoToml = ../Cargo.toml; }) version;

          inherit src strictDeps;
        };
      };
    };
}
