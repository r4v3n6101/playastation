# Author and source: https://github.com/PeterLemon/PSX/
{
  pkgs,
  test-rom-runner,
  bios,
  test-dir,
}:
pkgs.writeShellApplication {
  name = "rom-cpu-tests";

  text = ''
    mkdir -p output

    trap 'kill 0; exit 130' INT TERM

    while IFS= read -r -d "" asm; do
        exe="$(dirname "$asm")/$(basename "$asm" .asm).exe"
        ${test-rom-runner}/bin/test-rom-runner \
          "${bios}" \
          "$exe" &
    done < <(find "${test-dir}" -type f -name '*.asm' -print0)

    wait
  '';
}
