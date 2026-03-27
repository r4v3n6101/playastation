use cop0::{Cop0, Exception};
use pipeline::{ErrorKind as PipelineErrorKind, Pipeline};

use crate::interconnect::{Bus, BusError, BusErrorKind};

mod cop0;
mod pipeline;

#[derive(Debug)]
pub struct Cpu {
    pub regs: Registers,
    pub pipeline: Pipeline,
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
            pipeline: Pipeline::default(),
            cop0: Cop0::default(),
        }
    }
}

impl Cpu {
    pub fn cycle(&mut self, bus: &mut Bus) {
        let fetch = self.pipeline.fetch(&mut self.regs.pc, &self.cop0, bus);
        let decode = self.pipeline.decode(&self.regs);
        let execute = self.pipeline.execute(&mut self.regs.pc, &mut self.cop0);
        let mem = self.pipeline.memory(bus, &mut self.cop0);
        self.pipeline.writeback(&mut self.regs);

        let (err, flush_count) = if let Err(err) = mem {
            (err, 4)
        } else if let Err(err) = execute {
            (err, 3)
        } else if let Err(err) = decode {
            (err, 2)
        } else if let Err(err) = fetch {
            (err, 1)
        } else {
            return;
        };

        let has_delay_slot = self.pipeline.flush(flush_count);

        let exception = match err.kind {
            PipelineErrorKind::InvalidInstruction(_) => Exception::ReservedInstruction,
            PipelineErrorKind::AluOverflow => Exception::Overflow,
            PipelineErrorKind::Break => Exception::Break,
            PipelineErrorKind::Syscall => Exception::Syscall,
            PipelineErrorKind::Interrupt => Exception::Interrupt,

            PipelineErrorKind::MemoryLoad(BusError {
                bad_vaddr,
                kind: BusErrorKind::UnalignedAddr,
            })
            | PipelineErrorKind::InsLoad(BusError {
                bad_vaddr,
                kind: BusErrorKind::UnalignedAddr,
            }) => Exception::UnalignedLoad { bad_vaddr },

            PipelineErrorKind::MemoryStore(BusError {
                bad_vaddr,
                kind: BusErrorKind::UnalignedAddr,
            }) => Exception::UnalignedStore { bad_vaddr },

            PipelineErrorKind::InsLoad(BusError { bad_vaddr, .. }) => {
                Exception::InstructionBus { bad_vaddr }
            }
            PipelineErrorKind::MemoryLoad(BusError { bad_vaddr, .. })
            | PipelineErrorKind::MemoryStore(BusError { bad_vaddr, .. }) => {
                Exception::DataBus { bad_vaddr }
            }
        };

        self.cop0.exception_enter(exception, err.pc, has_delay_slot);
        self.regs.pc = self.cop0.exception_handler();
    }
}
