use std::{fs, process::Command};

use playastation::{cpu::Cpu, mem::Bus};

fn create_and_run_program(name: &'static str) -> (Cpu, Bus) {
    let mut bus = Bus::default();
    let mut cpu = Cpu::default();
    cpu.regs.pc = 0;

    let output = format!("{name}.bin");
    fs::File::create(&output).unwrap();
    Command::new("armips")
        .arg(format!("tests/asm/{name}.s"))
        .spawn()
        .unwrap()
        .wait()
        .unwrap();
    let program = fs::read(&output).unwrap();
    bus.mem.storage[..program.len()].copy_from_slice(&program[..]);
    fs::remove_file(output).unwrap();

    for _ in 0..1000 {
        cpu.cycle(&mut bus);
    }

    (cpu, bus)
}

#[test]
fn test_basic_alu() {
    let (cpu, _) = create_and_run_program("alu_basic");

    assert_eq!(cpu.regs.general[1], 5);
    assert_eq!(cpu.regs.general[2], 7);
    assert_eq!(cpu.regs.general[3], 12);
    assert_eq!(cpu.regs.general[4], 7);
    assert_eq!(cpu.regs.general[5], 7);
    assert_eq!(cpu.regs.general[6], 5);
    assert_eq!(cpu.regs.general[7], 2);
}

#[test]
fn test_basic_shift() {
    let (cpu, _) = create_and_run_program("shift_basic");

    assert_eq!(cpu.regs.general[2], 16);
    assert_eq!(cpu.regs.general[3], 4);
    assert_eq!(cpu.regs.general[4], 4);
    assert_eq!(cpu.regs.general[6], 8);
}

#[test]
fn test_mem_basic() {
    let (cpu, bus) = create_and_run_program("mem_basic");

    assert_eq!(bus.read_word(0x1000), 0x00001234);
    assert_eq!(cpu.regs.general[3], 0x00001234);
}

#[test]
fn test_mem_offset_signed() {
    let (cpu, bus) = create_and_run_program("mem_offset_signed");

    assert_eq!(bus.read_word(0x1000), 99);
    assert_eq!(cpu.regs.general[3], 99);
}

#[test]
fn test_branch_not_taken() {
    let (cpu, _) = create_and_run_program("branch_not_taken");

    assert_eq!(cpu.regs.general[3], 11);
    assert_eq!(cpu.regs.general[4], 22);
    assert_eq!(cpu.regs.general[5], 33);
}

#[test]
fn test_branch_taken_delay_slot() {
    let (cpu, _) = create_and_run_program("branch_taken_delay_slot");

    assert_eq!(cpu.regs.general[3], 11);
    assert_eq!(cpu.regs.general[4], 0);
    assert_eq!(cpu.regs.general[5], 33);
}

#[test]
fn test_branch_backward_loop() {
    let (cpu, _) = create_and_run_program("branch_backward_loop");

    assert_eq!(cpu.regs.general[1], 0);
    assert_eq!(cpu.regs.general[2], 3);
}

#[test]
fn test_jump_delay_slot() {
    let (cpu, _) = create_and_run_program("jump_delay_slot");

    assert_eq!(cpu.regs.general[1], 77);
    assert_eq!(cpu.regs.general[2], 0);
    assert_eq!(cpu.regs.general[3], 99);
}

#[test]
fn test_jal_jr_return() {
    let (cpu, _) = create_and_run_program("jal_jr_return");

    assert_eq!(cpu.regs.general[10], 1);
    assert_eq!(cpu.regs.general[11], 2);
    assert_eq!(cpu.regs.general[12], 3);
    assert_eq!(cpu.regs.general[13], 4);
    assert_eq!(cpu.regs.general[31], 8);
}

#[test]
fn test_load_use_hazard() {
    let (cpu, _) = create_and_run_program("load_use_hazard");

    // Because of old data caused by delay-slot
    assert_ne!(cpu.regs.general[4], 110);
}

#[test]
fn test_alu_to_branch_dep() {
    let (cpu, _) = create_and_run_program("alu_to_branch_dep");

    assert_eq!(cpu.regs.general[4], 1);
    assert_eq!(cpu.regs.general[5], 0);
    assert_eq!(cpu.regs.general[6], 3);
}
