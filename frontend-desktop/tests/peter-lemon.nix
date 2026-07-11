# Author and source: https://github.com/PeterLemon/PSX/
{ ... }: {
  perSystem =
    {
      pkgs,
      bios,
      self',
      ...
    }:
    let
      test-roms = pkgs.fetchFromGitHub {
        owner = "PeterLemon";
        repo = "PSX";
        rev = "6d20c132aba02cf387ed2224993ce9ee9a48e620";
        fetchSubmodules = true;
        hash = "sha256-ZZEONU0Qzh1LO0UOVugPdjyGNXT03zfgMKSj4VnKa4k=";
      };

      test-suite =
        dir:
        pkgs.writeShellApplication {
          name = "peter-lemon-tests";

          runtimeInputs = [ self'.packages.frontend-desktop ];

          text = ''
            while IFS= read -r -d "" exe; do
              playastation-desktop \
                --bios "${bios}" \
                --rom "$exe"
            done < <(find "${test-roms}/${dir}" -type f \( -name '*.exe' -o -name '*.EXE' \) -print0)
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
