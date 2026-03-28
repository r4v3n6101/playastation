use std::{fs, process::Command};

use playastation::{
    cpu::{Cause, Cpu},
    interconnect::Bus,
};

fn create_and_run_program(name: &'static str, cycles: usize) -> (Cpu, Bus) {
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
    bus.ram_mut()[..program.len()].copy_from_slice(&program);
    fs::remove_file(output).unwrap();

    for _ in 0..cycles {
        cpu.cycle(&mut bus);
    }

    (cpu, bus)
}

#[test]
fn test_basic_alu() {
    let (cpu, _) = create_and_run_program("alu_basic", 11);

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
    let (cpu, _) = create_and_run_program("shift_basic", 100);

    assert_eq!(cpu.regs.general[2], 16);
    assert_eq!(cpu.regs.general[3], 4);
    assert_eq!(cpu.regs.general[4], 4);
    assert_eq!(cpu.regs.general[6], 8);
}

#[test]
fn test_mem_basic() {
    let (cpu, bus) = create_and_run_program("mem_basic", 100);

    assert_eq!(bus.read_word(0x1000).unwrap(), 0x00001234);
    assert_eq!(cpu.regs.general[3], 0x00001234);
}

#[test]
fn test_mem_offset_signed() {
    let (cpu, bus) = create_and_run_program("mem_offset_signed", 100);

    assert_eq!(bus.read_word(0x1000).unwrap(), 99);
    assert_eq!(cpu.regs.general[3], 99);
}

#[test]
fn test_branch_not_taken() {
    let (cpu, _) = create_and_run_program("branch_not_taken", 100);

    assert_eq!(cpu.regs.general[3], 11);
    assert_eq!(cpu.regs.general[4], 22);
    assert_eq!(cpu.regs.general[5], 33);
}

#[test]
fn test_branch_taken_delay_slot() {
    let (cpu, _) = create_and_run_program("branch_taken_delay_slot", 100);

    assert_eq!(cpu.regs.general[3], 11);
    assert_eq!(cpu.regs.general[4], 0);
    assert_eq!(cpu.regs.general[5], 33);
}

#[test]
fn test_branch_backward_loop() {
    let (cpu, _) = create_and_run_program("branch_backward_loop", 100);

    assert_eq!(cpu.regs.general[1], 0);
    assert_eq!(cpu.regs.general[2], 3);
}

#[test]
fn test_jump_delay_slot() {
    let (cpu, _) = create_and_run_program("jump_delay_slot", 100);

    assert_eq!(cpu.regs.general[1], 77);
    assert_eq!(cpu.regs.general[2], 0);
    assert_eq!(cpu.regs.general[3], 99);
}

#[test]
fn test_jal_jr_return() {
    let (cpu, _) = create_and_run_program("jal_jr_return", 100);

    assert_eq!(cpu.regs.general[10], 1);
    assert_eq!(cpu.regs.general[11], 2);
    assert_eq!(cpu.regs.general[12], 3);
    assert_eq!(cpu.regs.general[13], 4);
    assert_eq!(cpu.regs.general[31], 8);
}

#[test]
fn test_load_use_hazard() {
    let (cpu, _) = create_and_run_program("load_use_hazard", 100);

    // Because of old data caused by delay-slot
    assert_ne!(cpu.regs.general[4], 110);
}

#[test]
fn test_alu_to_branch_dep() {
    let (cpu, _) = create_and_run_program("alu_to_branch_dep", 100);

    assert_eq!(cpu.regs.general[4], 1);
    assert_eq!(cpu.regs.general[5], 0);
    assert_eq!(cpu.regs.general[6], 3);
}

#[test]
fn test_syscall_exception() {
    let (cpu, _) = create_and_run_program("syscall_exception", 3);

    assert_eq!(cpu.cop0.regs[14], 0);
    assert_eq!(cpu.cop0.cause_reg().read(Cause::EXCCODE), 8);
}

#[test]
fn test_break_exception() {
    let (cpu, _) = create_and_run_program("break_exception", 3);

    assert_eq!(cpu.cop0.regs[14], 0);
    assert_eq!(cpu.cop0.cause_reg().read(Cause::EXCCODE), 9);
}

#[test]
fn test_overflow_exception() {
    let (cpu, _) = create_and_run_program("overflow_exception", 6);

    assert_eq!(cpu.regs.general[3], 0);
    assert_eq!(cpu.cop0.regs[14], 12);
    assert_eq!(cpu.cop0.cause_reg().read(Cause::EXCCODE), 12);
}

#[test]
fn test_reserved_instruction_exception() {
    let (cpu, _) = create_and_run_program("reserved_instruction_exception", 2);

    assert_eq!(cpu.cop0.regs[14], 0);
    assert_eq!(cpu.cop0.cause_reg().read(Cause::EXCCODE), 10);
}

#[test]
fn test_div_normal() {
    let (cpu, _) = create_and_run_program("div_normal", 100);

    assert_eq!(cpu.regs.general[3], 3);
    assert_eq!(cpu.regs.general[4], 1);
}

#[test]
fn test_div_by_zero() {
    let (cpu, _) = create_and_run_program("div_by_zero", 100);

    // No exception
    assert_eq!(cpu.cop0.cause_reg().read(Cause::EXCCODE), 0);
}

#[test]
fn test_delay_slot_exception_bd() {
    let (cpu, _) = create_and_run_program("delay_slot_exception_bd", 4);

    assert_eq!(cpu.cop0.regs[14], 0);
    assert_eq!(cpu.cop0.cause_reg().read(Cause::EXCCODE), 8);
    assert!(cpu.cop0.cause_reg().is_set(Cause::BD));
}

#[test]
fn test_misaligned_lw_exception() {
    let (cpu, _) = create_and_run_program("misaligned_lw_exception", 6);

    assert_eq!(cpu.cop0.regs[14], 8);
    assert_eq!(cpu.cop0.cause_reg().read(Cause::EXCCODE), 4);
}

#[test]
fn test_misaligned_sw_exception() {
    let (cpu, _) = create_and_run_program("misaligned_sw_exception", 7);

    assert_eq!(cpu.cop0.regs[14], 12);
    assert_eq!(cpu.cop0.cause_reg().read(Cause::EXCCODE), 5);
}

#[test]
fn test_mtc0_basic() {
    let (cpu, _) = create_and_run_program("mtc0_basic", 100);

    assert_eq!(cpu.cop0.regs[14], 0x1234_5678);
}

#[test]
fn test_mfc0_basic() {
    let (cpu, _) = create_and_run_program("mfc0_basic", 12);

    assert_eq!(cpu.regs.general[3], 0x1234_5678);
}
