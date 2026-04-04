use std::{fs, time::Instant};

use playastation::{cpu::Cpu, interconnect::Bus};

#[test]
#[ignore = "Time consuming"]
fn test_bios_smoke_run() {
    let bios = fs::read(env!("PSX_BIOS")).expect("failed to read BIOS ROM");
    assert_eq!(bios.len(), 512 * 1024, "unexpected BIOS size");

    let mut bus = Bus::default();
    bus.bios[..bios.len()].copy_from_slice(&bios);

    let mut cpu = Cpu::default();

    let instant = Instant::now();
    for i in 0..1_000_000_000 {
        cpu.cycle(&mut bus);
        if 1_000_000_000 - i < 1_000_000 {
            println!("{cpu:?}");
        }
    }

    println!("Executed in {:?}", instant.elapsed());

    println!(
        "Cause: {:?}, Status: {:?}, bad_vaddr: {}",
        cpu.cop0.cause(),
        cpu.cop0.status(),
        cpu.cop0.regs[8]
    );
}
