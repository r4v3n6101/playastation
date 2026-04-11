use std::{fs, time::Instant};

use playastation::{
    interconnect::Bus,
    run::{CpuExecutor, interpreter::Interpreter},
};

#[test]
#[ignore = "Time consuming"]
fn test_bios_smoke_run() {
    let bios = fs::read(env!("PSX_BIOS")).expect("failed to read BIOS ROM");
    assert_eq!(bios.len(), 512 * 1024, "unexpected BIOS size");

    let mut bus = Bus::default();
    bus.bios[..bios.len()].copy_from_slice(&bios);

    let mut executor = CpuExecutor::<Interpreter>::default();

    let instant = Instant::now();
    for _ in 0..33868800 {
        executor.cycle(&mut bus);
    }

    println!("Executed in {:?}", instant.elapsed());

    println!(
        "Cause: {:?}, Status: {:?}, bad_vaddr: {}",
        executor.cpu.cop0.cause(),
        executor.cpu.cop0.status(),
        executor.cpu.cop0.regs[8]
    );
}
