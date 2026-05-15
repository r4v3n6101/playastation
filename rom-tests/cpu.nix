{
  pkgs,
  runner,
  psx-tests,
}:
let
  tests = [
    "CPUTest/CPU/XOR/CPUXOR.bin"
    "CPUTest/CPU/ADDIU/CPUADDIU.bin"
    "CPUTest/CPU/DIV/CPUDIV.bin"
    "CPUTest/CPU/NOR/CPUNOR.bin"
    "CPUTest/CPU/ORI/CPUORI.bin"
    "CPUTest/CPU/MULTU/CPUMULTU.bin"
    "CPUTest/CPU/SUBU/CPUSUBU.bin"
    "CPUTest/CPU/XORI/CPUXORI.bin"
    "CPUTest/CPU/SHIFT/SLLV/CPUSLLV.bin"
    "CPUTest/CPU/SHIFT/SLL/CPUSLL.bin"
    "CPUTest/CPU/SHIFT/SRA/CPUSRA.bin"
    "CPUTest/CPU/SHIFT/SRAV/CPUSRAV.bin"
    "CPUTest/CPU/SHIFT/SRL/CPUSRL.bin"
    "CPUTest/CPU/SHIFT/SRLV/CPUSRLV.bin"
    "CPUTest/CPU/SUB/CPUSUB.bin"
    "CPUTest/CPU/AND/CPUAND.bin"
    "CPUTest/CPU/ADD/CPUADD.bin"
    "CPUTest/CPU/DIVU/CPUDIVU.bin"
    "CPUTest/CPU/OR/CPUOR.bin"
    "CPUTest/CPU/LOADSTORE/SB/CPUSB.bin"
    "CPUTest/CPU/LOADSTORE/SW/CPUSW.bin"
    "CPUTest/CPU/LOADSTORE/LB/CPULB.bin"
    "CPUTest/CPU/LOADSTORE/LW/CPULW.bin"
    "CPUTest/CPU/LOADSTORE/SH/CPUSH.bin"
    "CPUTest/CPU/LOADSTORE/LH/CPULH.bin"
    "CPUTest/CPU/ADDU/CPUADDU.bin"
    "CPUTest/CPU/ANDI/CPUANDI.bin"
    "CPUTest/CPU/ADDI/CPUADDI.bin"
    "CPUTest/CPU/MULT/CPUMULT.bin"
  ];
in
pkgs.writeShellApplication {
  name = "rom-cpu-tests";

  text =
    let
      runOne = test: ''
        ${runner}/bin/test-rom-runner ${psx-tests}/${test} &
      '';
    in
    ''
      mkdir -p output
      ${pkgs.lib.concatMapStringsSep "\n" runOne tests}
    '';
}
