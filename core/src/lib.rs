// Used for software renderer multi-threading
#![cfg_attr(not(feature = "software-renderer"), no_std)]

extern crate alloc;

pub mod cpu;
pub mod devices;
pub mod globals;
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
        assert_eq!(bios.len(), globals::BIOS_SIZE, "invalid bios size");
        self.bus.bios.copy_from_slice(bios);
    }

    pub fn set_render(&mut self, renderer: impl render::Renderer) {
        self.bus.gpu.renderer = alloc::boxed::Box::new(renderer);
    }

    pub fn run(mut self) {
        loop {
            self.executor.run(&mut self.bus);
        }
    }
}
