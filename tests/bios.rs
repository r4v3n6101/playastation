use std::{fs, panic::catch_unwind, time::Instant};

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

    let now = Instant::now();
    println!("Starts at {now:?}");

    let _ = catch_unwind(move || {
        println!("Executed at {:?}", Instant::now());
    });

    loop {
        executor.run(&mut bus);
    }
}
