use cop0::{Cop0, Exception};
use pipeline::{ErrorKind as PipelineErrorKind, Pipeline};

use crate::mem;

mod cop0;
mod ins;
mod pipeline;

#[derive(Debug)]
pub struct Cpu {
    pub regs: Registers,
    pub pipeline: Pipeline,
    pub cop0: Cop0,
}

#[derive(Debug, Clone)]
pub struct Registers {
    pub general: [u32; 32],
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
                pc: 0xBFC0_0000,
                hi: 0,
                lo: 0,
            },
            pipeline: Pipeline::default(),
            cop0: Cop0::default(),
        }
    }
}

impl Cpu {
    pub fn cycle(&mut self, bus: &mut mem::Bus) {
        let fetch = self.pipeline.fetch(&mut self.regs.pc, bus);
        let decode = self.pipeline.decode(&self.regs);
        self.pipeline.execute(&mut self.regs.pc);
        let mem = self.pipeline.memory(bus);
        self.pipeline.writeback(&mut self.regs);

        // TODO : log errors
        let (err, flush_count) = if let Err(err) = mem {
            (err, 4)
        } else if let Err(err) = decode {
            (err, 2)
        } else if let Err(err) = fetch {
            (err, 1)
        } else {
            return;
        };

        let (fault_pc, has_delay_slot) = self
            .pipeline
            .flush(flush_count)
            .map_or((err.pc, false), |res| (res, true));
        let exception = match err.kind {
            PipelineErrorKind::InvalidInstruction(_) => Exception::ReservedInstruction,
            PipelineErrorKind::AluOverflow => Exception::Overflow,
            PipelineErrorKind::MemoryStore(mem::Error { bad_vaddr, .. }) => {
                Exception::AddressStore { bad_vaddr }
            }
            PipelineErrorKind::MemoryLoad(mem::Error { bad_vaddr, .. }) => {
                Exception::AddressLoad { bad_vaddr }
            }
            PipelineErrorKind::Break => Exception::Break,
            PipelineErrorKind::Syscall => Exception::Syscall,
        };
        self.cop0
            .exception_enter(exception, fault_pc, has_delay_slot, &mut self.regs.pc);
    }
}
