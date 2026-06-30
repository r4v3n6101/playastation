# Author and source: https://github.com/nicolasnoble/pcsx-redux/
{ inputs, ... }:
{
  perSystem =
    { pkgs, self', ... }:
    let
      mipselPkgs = import pkgs.path {
        inherit (pkgs.stdenv.hostPlatform) system;

        crossSystem = {
          config = "mipsel-none-elf";
          gcc = {
            arch = "mips1";
            tune = "r3000";
          };
        };
      };

      test-rom =
        kind:
        mipselPkgs.stdenv.mkDerivation {
          name = "pcsx-redux-test-rom";

          src = pkgs.fetchFromGitHub {
            owner = "nicolasnoble";
            repo = "pcsx-redux";
            rev = "221e96bdbd9bf52e7af631864aa22b9b0513581e";
            fetchSubmodules = true;
            hash = "sha256-hVc2jH8Nr8iA7PPm5c6t7DSVSJKRt2cr48uiYiUtpBI=";
          };

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

      test-suite =
        kind:
        pkgs.writeShellApplication {
          name = "pcsx-redux-tests";

          runtimeInputs = [ self'.packages.frontend-desktop ];

          text = ''
            playastation-desktop \
              --bios "${inputs.bios}" \
              --rom "${(test-rom kind)}"
          '';
        };
    in
    {
      legacyPackages.pcsx-redux-tests = pkgs.lib.makeScope pkgs.newScope (_: {
        basic = test-suite "basic";
        cop0 = test-suite "cop0";
        cpu = test-suite "cpu";
        gpu = test-suite "gpu";
        timers = test-suite "timers";
      });
    };
}
