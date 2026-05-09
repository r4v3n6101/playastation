pub mod cpu;
pub mod devices;
pub mod interconnect;
pub mod render;
pub mod run;

#[derive(Default)]
pub struct Console {
    pub executor: run::CpuExecutor,
    pub bus: interconnect::Bus,
}

impl Console {
    pub fn load_bios(&mut self, bios: &[u8]) {
        // 512KiB
        const BIOS_SIZE: usize = 512 * 1024;

        assert_eq!(bios.len(), BIOS_SIZE, "invalid bios size");
        self.bus.bios.copy_from_slice(bios);
    }

    pub fn run(&mut self) {
        self.executor.run(&mut self.bus);
    }
}
