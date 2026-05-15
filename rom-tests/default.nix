{
  pkgs,
  runner,
  test-dir,
}:
pkgs.writeShellApplication {
  name = "rom-cpu-tests";

  runtimeInputs = [
    pkgs.coreutils
  ];

  text = ''
    mkdir -p output

    trap 'kill 0' INT

    find ${test-dir} -type f -name '*.asm' -print0 |
    while IFS= read -r -d "" asm; do
        timeout 30s ${runner}/bin/test-rom-runner \
          "$(dirname "$asm")/$(basename "$asm" .asm).bin" &
    done
  '';
}
