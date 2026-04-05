use crate::interconnect::Bus;

pub use cop0::{Cop0, Exception};

mod cop0;
mod ins;
mod pipeline;

#[derive(Debug)]
pub struct Cpu {
    pipeline: pipeline::State,
    pub regs: Registers,
    pub cop0: Cop0,
}

#[derive(Debug, Copy, Clone)]
pub struct Registers {
    pub general: [u32; 32],
    pub pc: u32,
    pub hi: u32,
    pub lo: u32,
}

/// Reset state of the CPU.
impl Default for Cpu {
    fn default() -> Self {
        Self {
            regs: Registers {
                general: [0; _],
                pc: 0xBFC0_0000,
                hi: 0,
                lo: 0,
            },
            pipeline: Default::default(),
            cop0: Default::default(),
        }
    }
}

impl Cpu {
    pub fn cycle(&mut self, bus: &mut Bus) {
        self.cop0.set_hw_irq(bus.int_ctrl.pending());

        if let Err((has_delay_slot, fault_pc, exception)) =
            self.pipeline.run(&mut self.regs, &mut self.cop0, bus)
        {
            self.cop0
                .exception_enter(exception, fault_pc, has_delay_slot);
            self.regs.pc = self.cop0.exception_handler();
        }
    }
}
