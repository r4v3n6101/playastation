use cop0::Cop0;
use pipeline::Pipeline;

use crate::mem::Bus;

mod cop0;
mod ins;
mod pipeline;

#[derive(Debug)]
pub struct Cpu {
    pub regs: Registers,
    pub pipeline: Pipeline,
    pub cop0: Cop0,
}

#[derive(Debug)]
pub struct Registers {
    pub general: [u32; 32],
    // TODO : it's not 0x0, rather
    pub pc: u32,
    pub hi: u32,
    pub lo: u32,
}

/// Reset the state of the CPU.
impl Default for Cpu {
    fn default() -> Self {
        Self {
            regs: Registers {
                general: [0; _],
                pc: 0xBFC00000,
                hi: 0,
                lo: 0,
            },
            pipeline: Pipeline::default(),
            cop0: Cop0::default(),
        }
    }
}

impl Cpu {
    pub fn cycle(&mut self, bus: &mut Bus) {
        self.pipeline.fetch(&mut self.regs, bus);
        self.pipeline.decode(&mut self.regs);
        self.pipeline.execute(&mut self.regs);
        self.pipeline.memory(bus);
        self.pipeline.writeback(&mut self.regs);
    }
}
