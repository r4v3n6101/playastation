use std::fs;

use playastation::{
    interconnect::Bus,
    run::{CpuExecutor, interpreter::Interpreter},
};

fn main() {
    let bios = fs::read(env!("PSX_BIOS")).expect("failed to read BIOS ROM");
    assert_eq!(bios.len(), 512 * 1024, "unexpected BIOS size");

    let mut bus = Bus::default();
    bus.bios[..bios.len()].copy_from_slice(&bios);

    let mut executor = CpuExecutor::<Interpreter>::default();

    loop {
        executor.run(&mut bus);
    }
}
