use std::cell::UnsafeCell;

use crate::{cpu::Cpu, interconnect::Bus};

use super::{super::decoder::InsIter, ExecutionResult, FuncResult, compile_fn};

fn compile_and_run(cpu: &mut Cpu, bus: &mut Bus, words: &[(u32, u32)]) -> FuncResult {
    let storage = UnsafeCell::default();

    words.iter().for_each(|&(addr, val)| {
        let _ = bus.store(addr, val.to_le_bytes());
    });

    // Storage called once
    let func = unsafe {
        let mut pc = 0;
        compile_fn(
            &storage,
            0,
            InsIter::new_start_from(&mut pc, bus, words.len()),
        )
    };

    let mut res = Default::default();
    func.call(&mut res, cpu, bus);
    res
}

#[test]
fn compiles_and_executes_alu_block() {
    let mut cpu = Cpu::default();
    let mut bus = Bus::default();

    let res = compile_and_run(
        &mut cpu,
        &mut bus,
        &[
            (0x0000_0000, 0x2408_0005), // addiu t0, zero, 5
            (0x0000_0004, 0x2509_0007), // addiu t1, t0, 7
            (0x0000_0008, 0x0109_5021), // addu  t2, t0, t1
            (0x0000_000C, 0x2400_0001), // addiu zero, zero, 1
        ],
    );

    assert_eq!(cpu.gpr[8], 5);
    assert_eq!(cpu.gpr[9], 12);
    assert_eq!(cpu.gpr[10], 17);
    assert_eq!(cpu.gpr[0], 0);

    assert_eq!(
        res,
        FuncResult {
            result: ExecutionResult::Success,
            last_pc: 0x0000_000C,
            last_in_delay_slot: 0,
            bad_vaddr: 0,
            jump_addr: 0,
        }
    );
}

#[test]
fn stops_on_overflow_and_preserves_destination_register() {
    let mut cpu = Cpu::default();
    let mut bus = Bus::default();

    let res = compile_and_run(
        &mut cpu,
        &mut bus,
        &[
            (0x0000_0000, 0x3C08_7FFF), // lui   t0, 0x7fff
            (0x0000_0004, 0x3508_FFFF), // ori   t0, t0, 0xffff
            (0x0000_0008, 0x2108_0001), // addi  t0, t0, 1
            (0x0000_000C, 0x2409_0001), // addiu t1, zero, 1
        ],
    );

    assert_eq!(cpu.gpr[8], 0x7FFF_FFFF);
    assert_eq!(cpu.gpr[9], 0);

    assert_eq!(
        res,
        FuncResult {
            result: ExecutionResult::Overflow,
            last_pc: 0x0000_0008,
            last_in_delay_slot: 0,
            bad_vaddr: 0,
            jump_addr: 0,
        }
    );
}

#[test]
fn applies_load_delay_and_handles_nested_loads() {
    let mut cpu = Cpu::default();
    let mut bus = Bus::default();

    bus.store(0x20, 0x1111_1111u32.to_le_bytes()).unwrap();
    bus.store(0x24, 0x2222_2222u32.to_le_bytes()).unwrap();

    let res = compile_and_run(
        &mut cpu,
        &mut bus,
        &[
            (0x0000_0000, 0x2408_0020), // addiu t0, zero, 0x20
            (0x0000_0004, 0x8D09_0000), // lw    t1, 0(t0)
            (0x0000_0008, 0x0120_5021), // addu  t2, t1, zero
            (0x0000_000C, 0x8D09_0004), // lw    t1, 4(t0)
            (0x0000_0010, 0x0120_5821), // addu  t3, t1, zero
            (0x0000_0014, 0x0120_6021), // addu  t4, t1, zero
        ],
    );

    // First dependent instruction must still see the old t1 value.
    assert_eq!(cpu.gpr[10], 0);
    // After the first delay slot, the first load becomes visible.
    assert_eq!(cpu.gpr[11], 0x1111_1111);
    // After the nested load delay slot, the second load becomes visible.
    assert_eq!(cpu.gpr[9], 0x2222_2222);
    assert_eq!(cpu.gpr[12], 0x2222_2222);

    assert_eq!(
        res,
        FuncResult {
            result: ExecutionResult::Success,
            last_pc: 0x0000_0014,
            last_in_delay_slot: 0,
            bad_vaddr: 0,
            jump_addr: 0,
        }
    );
}

#[test]
fn second_load_uses_old_base_when_it_depends_on_previous_load() {
    let mut cpu = Cpu::default();
    let mut bus = Bus::default();

    bus.store(0x40, 0x1111_1111u32.to_le_bytes()).unwrap();
    bus.store(0x20, 0x0000_0040u32.to_le_bytes()).unwrap();
    bus.store(0x30, 0x2222_2222u32.to_le_bytes()).unwrap();

    let res = compile_and_run(
        &mut cpu,
        &mut bus,
        &[
            (0x0000_0000, 0x2408_0020), // addiu t0, zero, 0x20
            (0x0000_0004, 0x8D09_0000), // lw    t1, 0(t0)
            (0x0000_0008, 0x8D2A_0000), // lw    t2, 0(t1)
            (0x0000_000C, 0x0120_5821), // addu  t3, t1, zero
            (0x0000_0010, 0x0140_6021), // addu  t4, t2, zero
        ],
    );

    // The first load becomes visible only after the second load has already
    // computed its address, so the nested load must use the old t1 value (0).
    assert_eq!(cpu.gpr[8], 0x0000_0020); // t0
    assert_eq!(cpu.gpr[9], 0x0000_0040); // t1
    assert_eq!(cpu.gpr[10], 0x2408_0020); // t2
    assert_eq!(cpu.gpr[11], 0x0000_0040); // t3
    assert_eq!(cpu.gpr[12], 0x2408_0020); // t4

    assert_eq!(
        res,
        FuncResult {
            result: ExecutionResult::Success,
            last_pc: 0x0000_0010,
            last_in_delay_slot: 0,
            bad_vaddr: 0,
            jump_addr: 0,
        }
    );
}

#[test]
fn taken_beq_returns_jump_and_executes_delay_slot() {
    let mut cpu = Cpu::default();
    let mut bus = Bus::default();

    let res = compile_and_run(
        &mut cpu,
        &mut bus,
        &[
            (0x0000_0000, 0x2408_0001), // addiu t0, zero, 1
            (0x0000_0004, 0x2409_0001), // addiu t1, zero, 1
            (0x0000_0008, 0x1109_0002), // beq   t0, t1, +2
            (0x0000_000C, 0x240A_0055), // addiu t2, zero, 0x55
            (0x0000_0010, 0x240B_0077), // addiu t3, zero, 0x77
        ],
    );

    assert_eq!(cpu.gpr[10], 0x55);
    assert_eq!(cpu.gpr[11], 0);

    assert_eq!(
        res,
        FuncResult {
            result: ExecutionResult::Jump,
            last_pc: 0x0000_000C,
            last_in_delay_slot: 1,
            bad_vaddr: 0,
            jump_addr: 0x0000_0014,
        }
    );
}

#[test]
fn not_taken_bne_still_executes_delay_slot_but_does_not_jump() {
    let mut cpu = Cpu::default();
    let mut bus = Bus::default();

    let res = compile_and_run(
        &mut cpu,
        &mut bus,
        &[
            (0x0000_0000, 0x2408_0001), // addiu t0, zero, 1
            (0x0000_0004, 0x2409_0001), // addiu t1, zero, 1
            (0x0000_0008, 0x1509_0002), // bne   t0, t1, +2
            (0x0000_000C, 0x240A_0066), // addiu t2, zero, 0x66
            (0x0000_0010, 0x240B_0077), // addiu t3, zero, 0x77
        ],
    );

    assert_eq!(cpu.gpr[10], 0x66);
    assert_eq!(cpu.gpr[11], 0);

    assert_eq!(
        res,
        FuncResult {
            result: ExecutionResult::Success,
            last_pc: 0x0000_000C,
            last_in_delay_slot: 1,
            bad_vaddr: 0,
            jump_addr: 0,
        }
    );
}

#[test]
fn jal_sets_ra_and_reports_jump_target() {
    let mut cpu = Cpu::default();
    let mut bus = Bus::default();

    let res = compile_and_run(
        &mut cpu,
        &mut bus,
        &[
            (0x0000_0000, 0x0C00_0008), // jal   0x20
            (0x0000_0004, 0x2408_0055), // addiu t0, zero, 0x55
        ],
    );

    assert_eq!(cpu.gpr[8], 0x55);
    assert_eq!(cpu.gpr[31], 0x0000_0008);

    assert_eq!(
        res,
        FuncResult {
            result: ExecutionResult::Jump,
            last_pc: 0x0000_0004,
            last_in_delay_slot: 1,
            bad_vaddr: 0,
            jump_addr: 0x0000_0020,
        }
    );
}

#[test]
fn jalr_uses_register_target_and_custom_link_register() {
    let mut cpu = Cpu::default();
    let mut bus = Bus::default();

    let res = compile_and_run(
        &mut cpu,
        &mut bus,
        &[
            (0x0000_0000, 0x2409_0024), // addiu t1, zero, 0x24
            (0x0000_0004, 0x0120_8009), // jalr  s0, t1
            (0x0000_0008, 0x240A_0077), // addiu t2, zero, 0x77
        ],
    );

    assert_eq!(cpu.gpr[10], 0x77);
    assert_eq!(cpu.gpr[16], 0x0000_000C);
    assert_eq!(cpu.gpr[31], 0);

    assert_eq!(
        res,
        FuncResult {
            result: ExecutionResult::Jump,
            last_pc: 0x0000_0008,
            last_in_delay_slot: 1,
            bad_vaddr: 0,
            jump_addr: 0x0000_0024,
        }
    );
}
