{
  pkgs,
  test-rom-runner,
  bios,
}:
pkgs.writeShellApplication {
  name = "bios-test";

  text = ''
    mkdir -p output

    trap 'kill 0' INT

    timeout 30s ${test-rom-runner}/bin/test-rom-runner \
      bios "${bios}" &

    wait
  '';
}
