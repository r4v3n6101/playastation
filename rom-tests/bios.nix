{
  pkgs,
  test-rom-runner,
  bios,
}:
pkgs.writeShellApplication {
  name = "bios-test";

  text = ''
    mkdir -p output

     ${test-rom-runner}/bin/test-rom-runner \
      bios "${bios}"
  '';
}
