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
    mkdir -p output

    ${test-rom-runner}/bin/test-rom-runner "${bios}" "${test-rom}"
  '';
}
