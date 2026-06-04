# Author: Amidog
# Source: https://psx.amidog.se/doku.php?id=psx:download:cpu
{
  pkgs,
  test-rom-runner,
  bios,
  test-rom,
}:
pkgs.writeShellApplication {
  name = "amidog-tests";

  text = ''
    ${test-rom-runner}/bin/test-rom-runner --bios "${bios}" --rom "${test-rom}"
  '';
}
