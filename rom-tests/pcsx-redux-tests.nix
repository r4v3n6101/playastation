# Author and source: https://github.com/nicolasnoble/pcsx-redux/
{
  pkgs,
  test-rom-runner,
  bios,
  pcsx-redux,
  kind,
}:
let
  mipselPkgs = import pkgs.path {
    inherit (pkgs.stdenv.hostPlatform) system;

    crossSystem = {
      config = "mipsel-none-elf";
      libc = "newlib";

      gcc = {
        arch = "mips1";
        tune = "r3000";
      };
    };
  };

  test-rom = mipselPkgs.stdenv.mkDerivation {
    pname = "psx-redux-test-rom";
    version = "unstable";

    src = pcsx-redux;

    buildPhase = ''
      runHook preBuild
      make -C src/mips/tests/${kind} all
      runHook postBuild
    '';

    installPhase = ''
      runHook preInstall
      cp -r src/mips/tests/${kind}/${kind}.ps-exe $out
      runHook postInstall
    '';
  };
in
pkgs.writeShellApplication {
  name = "pcsx-redux-tests";

  text = ''
    mkdir -p output

    ${test-rom-runner}/bin/test-rom-runner "${bios}" "${test-rom}"
  '';
}
