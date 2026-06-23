# Author and source: https://github.com/PeterLemon/PSX/
{ inputs, ... }: {
  perSystem =
    { pkgs, self', ... }:
    let
      test-suite =
        dir:
        pkgs.writeShellApplication {
          name = "peter-lemon-tests";

          runtimeInputs = [ self'.packages.test-rom-runner ];

          text = ''
            while IFS= read -r -d "" exe; do
              test-rom-runner \
                --bios "${inputs.bios}" \
                --rom "$exe"
            done < <(find "${inputs.peter-lemon-test-roms}/${dir}" -type f \( -name '*.exe' -o -name '*.EXE' \) -print0)
          '';
        };
    in
    {
      legacyPackages.peter-lemon-tests = pkgs.lib.makeScope pkgs.newScope (_: {
        cpu = test-suite "CPUTest/CPU/";
        gpu = test-suite "GPU/";
        cube = test-suite "Cube/";
        vblank = test-suite "Demo/vblank/";
      });
    };
}
