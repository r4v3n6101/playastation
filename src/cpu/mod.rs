use crate::interconnect::Bus;

pub use cop0::{Cop0, Exception};

mod cop0;
mod ins;
mod pipeline;
// TODO : feature flag
mod jit;

#[derive(Default)]
pub struct CpuCtx {
    pipeline: pipeline::State,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Cpu {
    pub regs: Registers,
    pub cop0: Cop0,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Registers {
    pub pc: u32,
    pub hi: u32,
    pub lo: u32,
    pub general: [u32; 32],
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
            cop0: Default::default(),
        }
    }
}

impl Cpu {
    pub fn run(&mut self, ctx: &mut CpuCtx, bus: &mut Bus) {
        self.cop0.set_hw_irq(bus.int_ctrl.pending());

        if let Err((has_delay_slot, fault_pc, exception)) = ctx.pipeline.run(self, bus) {
            self.cop0
                .exception_enter(exception, fault_pc, has_delay_slot);
            self.regs.pc = self.cop0.exception_handler();
        }
    }
}
