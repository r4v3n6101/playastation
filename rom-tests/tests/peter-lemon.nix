# Author and source: https://github.com/PeterLemon/PSX/
{ inputs, ... }: {
  perSystem =
    { pkgs, self', ... }:
    let
      test-suite =
        dir:
        pkgs.writeShellApplication {
          name = "peter-lemon-tests";

          text = ''
            while IFS= read -r -d "" asm; do
                dir="$(dirname "$asm")"
                base="$(basename "$asm" .asm)"
                exe="$dir/$base.exe"

                if [[ -f "$exe" ]]; then
                    ${self'.packages.test-rom-runner}/bin/test-rom-runner \
                      --bios "${inputs.bios}" \
                      --rom "$exe"
                fi
            done < <(find "${inputs.peter-lemon-test-roms}/${dir}" -type f -name '*.asm' -print0)
          '';
        };
    in
    {
      legacyPackages.peter-lemon-tests = pkgs.lib.makeScope pkgs.newScope (_: {
        cpu = test-suite "CPUTest/CPU/";
        gpu = test-suite "GPU/";
        cube = test-suite "Cube/";
      });
    };
}
