use std::{fs, process::Command};

use playastation::{
    cpu::Cpu,
    interconnect::Bus,
    run::{CpuExecutor, interpreter::Interpreter},
};

fn create_and_run_program(name: &'static str) -> (Cpu, Bus) {
    let mut bus = Bus::default();
    let mut executor = CpuExecutor::<Interpreter>::default();
    executor.cpu.pc = 0;

    let output = format!("{name}.bin");
    fs::File::create(&output).unwrap();
    Command::new("armips")
        .arg(format!("tests/asm/{name}.s"))
        .spawn()
        .unwrap()
        .wait()
        .unwrap();
    let program = fs::read(&output).unwrap();
    bus.ram[..program.len()].copy_from_slice(&program);
    fs::remove_file(output).unwrap();

    for _ in 0..3 {
        executor.run(&mut bus);
    }

    (executor.cpu, bus)
}

#[test]
fn test_basic_alu() {
    let (cpu, _) = create_and_run_program("alu_basic");

    assert_eq!(cpu.gpr[1], 5);
    assert_eq!(cpu.gpr[2], 7);
    assert_eq!(cpu.gpr[3], 12);
    assert_eq!(cpu.gpr[4], 7);
    assert_eq!(cpu.gpr[5], 7);
    assert_eq!(cpu.gpr[6], 5);
    assert_eq!(cpu.gpr[7], 2);
}

#[test]
fn test_basic_shift() {
    let (cpu, _) = create_and_run_program("shift_basic");

    assert_eq!(cpu.gpr[2], 16);
    assert_eq!(cpu.gpr[3], 4);
    assert_eq!(cpu.gpr[4], 4);
    assert_eq!(cpu.gpr[6], 8);
}

#[test]
fn test_mem_basic() {
    let (cpu, bus) = create_and_run_program("mem_basic");

    assert_eq!(
        u32::from_le_bytes(bus.load::<4>(0x1000).unwrap()),
        0x00001234
    );
    assert_eq!(cpu.gpr[3], 0x00001234);
}

#[test]
fn test_mem_offset_signed() {
    let (cpu, bus) = create_and_run_program("mem_offset_signed");

    assert_eq!(u32::from_le_bytes(bus.load::<4>(0x1000).unwrap()), 99);
    assert_eq!(cpu.gpr[3], 99);
}

#[test]
fn test_branch_not_taken() {
    let (cpu, _) = create_and_run_program("branch_not_taken");

    assert_eq!(cpu.gpr[3], 11);
    assert_eq!(cpu.gpr[4], 22);
    assert_eq!(cpu.gpr[5], 33);
}

#[test]
fn test_branch_taken_delay_slot() {
    let (cpu, _) = create_and_run_program("branch_taken_delay_slot");

    assert_eq!(cpu.gpr[3], 11);
    assert_eq!(cpu.gpr[4], 0);
    assert_eq!(cpu.gpr[5], 33);
}

#[test]
fn test_branch_backward_loop() {
    let (cpu, _) = create_and_run_program("branch_backward_loop");

    assert_eq!(cpu.gpr[1], 0);
    assert_eq!(cpu.gpr[2], 3);
}

#[test]
fn test_jump_delay_slot() {
    let (cpu, _) = create_and_run_program("jump_delay_slot");

    assert_eq!(cpu.gpr[1], 77);
    assert_eq!(cpu.gpr[2], 0);
    assert_eq!(cpu.gpr[3], 99);
}

#[test]
fn test_jal_jr_return() {
    let (cpu, _) = create_and_run_program("jal_jr_return");

    assert_eq!(cpu.gpr[10], 1);
    assert_eq!(cpu.gpr[11], 2);
    assert_eq!(cpu.gpr[12], 3);
    assert_eq!(cpu.gpr[13], 4);
    assert_eq!(cpu.gpr[31], 8);
}

#[test]
fn test_load_use_hazard() {
    let (cpu, _) = create_and_run_program("load_use_hazard");

    // Because of old data caused by delay-slot
    assert_ne!(cpu.gpr[4], 110);
}

#[test]
fn test_alu_to_branch_dep() {
    let (cpu, _) = create_and_run_program("alu_to_branch_dep");

    assert_eq!(cpu.gpr[4], 1);
    assert_eq!(cpu.gpr[5], 0);
    assert_eq!(cpu.gpr[6], 3);
}

#[test]
fn test_syscall_exception() {
    let (cpu, _) = create_and_run_program("syscall_exception");

    assert_eq!(cpu.cop0.regs[14], 0);
    assert_eq!(cpu.cop0.cause().excode(), 8);
}

#[test]
fn test_break_exception() {
    let (cpu, _) = create_and_run_program("break_exception");

    assert_eq!(cpu.cop0.regs[14], 0);
    assert_eq!(cpu.cop0.cause().excode(), 9);
}

#[test]
fn test_overflow_exception() {
    let (cpu, _) = create_and_run_program("overflow_exception");

    assert_eq!(cpu.gpr[3], 0);
    assert_eq!(cpu.cop0.regs[14], 12);
    assert_eq!(cpu.cop0.cause().excode(), 12);
}

#[test]
fn test_reserved_instruction_exception() {
    let (cpu, _) = create_and_run_program("reserved_instruction_exception");

    assert_eq!(cpu.cop0.regs[14], 0);
    assert_eq!(cpu.cop0.cause().excode(), 10);
}

#[test]
fn test_div_normal() {
    let (cpu, _) = create_and_run_program("div_normal");

    assert_eq!(cpu.gpr[3], 3);
    assert_eq!(cpu.gpr[4], 1);
}

#[test]
fn test_div_by_zero() {
    let (cpu, _) = create_and_run_program("div_by_zero");

    // No exception
    assert_eq!(cpu.cop0.cause().excode(), 0);
}

#[test]
fn test_delay_slot_exception_bd() {
    let (cpu, _) = create_and_run_program("delay_slot_exception_bd");

    assert_eq!(cpu.cop0.regs[14], 0);
    assert_eq!(cpu.cop0.cause().excode(), 8);
    assert!(cpu.cop0.cause().bd());
}

#[test]
fn test_misaligned_lw_exception() {
    let (cpu, _) = create_and_run_program("misaligned_lw_exception");

    assert_eq!(cpu.cop0.regs[14], 8);
    assert_eq!(cpu.cop0.cause().excode(), 4);
}

#[test]
fn test_misaligned_sw_exception() {
    let (cpu, _) = create_and_run_program("misaligned_sw_exception");

    assert_eq!(cpu.cop0.regs[14], 12);
    assert_eq!(cpu.cop0.cause().excode(), 5);
}

#[test]
fn test_mtc0_basic() {
    let (cpu, _) = create_and_run_program("mtc0_basic");

    assert_eq!(cpu.cop0.regs[14], 0x1234_5678);
}

#[test]
fn test_mfc0_basic() {
    let (cpu, _) = create_and_run_program("mfc0_basic");

    assert_eq!(cpu.gpr[3], 0x1234_5678);
}
