# Author and source: https://github.com/PeterLemon/PSX/
{
  pkgs,
  test-rom-runner,
  bios,
  test-dir,
}:
pkgs.writeShellApplication {
  name = "peter-lemon-tests";

  text = ''
    while IFS= read -r -d "" asm; do
        dir="$(dirname "$asm")"
        base="$(basename "$asm" .asm)"
        exe="$dir/$base.exe"

        if [[ -f "$exe" ]]; then
            ${test-rom-runner}/bin/test-rom-runner \
              --bios "${bios}" \
              --rom "$exe"
        fi
    done < <(find "${test-dir}" -type f -name '*.asm' -print0)
  '';
}
