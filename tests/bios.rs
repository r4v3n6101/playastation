use std::fs;

use playastation::{cpu::Cpu, interconnect::Bus};

#[test]
fn test_bios_smoke_run() {
    let bios = fs::read(env!("PSX_BIOS")).expect("failed to read BIOS ROM");
    assert_eq!(bios.len(), 512 * 1024, "unexpected BIOS size");

    let mut bus = Bus::default();
    bus.bios[..bios.len()].copy_from_slice(&bios);

    let mut cpu = Cpu::default();

    for _ in 0..10_000_000 {
        cpu.cycle(&mut bus);
    }

    println!(
        "Cause: {:?}, Status: {:?}, bad_vaddr: {}",
        cpu.cop0.cause(),
        cpu.cop0.status(),
        cpu.cop0.regs[8]
    );
}
