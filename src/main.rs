use std::fs;

use playastation::{
    interconnect::Bus,
    run::{CpuExecutor, interpreter::Interpreter},
};
use tracing::Level;

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .with_target(true)
        .with_level(true)
        .compact()
        .init();

    let bios = fs::read(env!("PSX_BIOS")).expect("failed to read BIOS ROM");
    assert_eq!(bios.len(), 512 * 1024, "unexpected BIOS size");

    let mut bus = Bus::default();
    bus.bios[..bios.len()].copy_from_slice(&bios);

    let mut executor = CpuExecutor::<Interpreter>::default();

    loop {
        executor.run(&mut bus);
    }
}
