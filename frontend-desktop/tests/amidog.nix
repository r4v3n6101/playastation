# Author: Amidog
# Source: https://psx.amidog.se/doku.php?id=psx:download:cpu
{ inputs, ... }: {
  perSystem =
    { pkgs, self', ... }:
    let
      test-rom-cpu = pkgs.fetchzip {
        url = "https://psx.amidog.se/lib/exe/fetch.php?media=psx:download:psxtest_cpu.zip";
        hash = "sha256-GRDi0PQyliMEvsxfwmtQDW4Hh6EHpSc7CCoozCBM2xA=";
        stripRoot = false;
      };

      test-rom-cpx = pkgs.fetchzip {
        url = "https://psx.amidog.se/lib/exe/fetch.php?media=psx:download:psxtest_cpx.zip";
        hash = "sha256-QgYKuo812yLyZvJ1GFTn5Je7EMUIqyLZ572HJv/fMa4=";
        stripRoot = false;
      };

      test-suite =
        rom:
        pkgs.writeShellApplication {
          name = "amidog-tests";

          runtimeInputs = [ self'.packages.frontend-desktop ];

          text = ''
            playastation-desktop \
              --bios "${inputs.bios}" \
              --rom "${rom}"
          '';
        };
    in
    {
      legacyPackages.amidog-tests = pkgs.lib.makeScope pkgs.newScope (_: {
        cpu = test-suite "${test-rom-cpu}/psxtest_cpu.exe";
        cpx = test-suite "${test-rom-cpx}/psxtest_cpx.exe";
      });
    };
}
