# Author: Amidog
# Source: https://psx.amidog.se/doku.php?id=psx:download:cpu
{ inputs, ... }: {
  perSystem =
    { pkgs, self', ... }:
    let
      test-suite =
        rom:
        pkgs.writeShellApplication {
          name = "amidog-tests";

          runtimeInputs = [ self'.packages.test-rom-runner ];

          text = ''
            test-rom-runner \
              --bios "${inputs.bios}" \
              --rom "${rom}"
          '';
        };
    in
    {
      legacyPackages.amidog-tests = pkgs.lib.makeScope pkgs.newScope (_: {
        cpu = test-suite "${inputs.amidog-cpu-test-rom}/psxtest_cpu.exe";
        cpx = test-suite "${inputs.amidog-cpx-test-rom}/psxtest_cpx.exe";
      });
    };
}
