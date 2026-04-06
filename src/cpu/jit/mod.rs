use crate::interconnect::Bus;

use super::Cpu;

mod compiler;
mod decoder;

pub type FuncPtr = fn(*mut FuncResult, *mut CpuAdditionalCtx, *mut Cpu, *mut Bus);

#[repr(C, packed)]
pub struct FuncResult {
    /// Result of function execution.
    result: ExecutionResult,
    /// PC of last executed instruction.
    pc: u32,
    /// Flag (0=false, 1=true) whether last executed instruction is in delay slot.
    in_delay_slot: u32,
    /// Number of executed instructions.
    count: u64,
    /// Filled in case of invalid memory ops (unaligned load/store, unmapped)
    bad_vaddr: u32,
}

#[repr(u32)]
pub enum ExecutionResult {
    Success = 0,
    Overflow = 1,
    UnalignedLoad = 2,
    UnalignedStore = 3,
    DataBus = 4,
}

/// Specific to JIT module context for CPU.
#[repr(C, packed)]
pub struct CpuAdditionalCtx {
    /// Slot for pending loads.
    /// If dest (1st arg) is 0, then no pending
    pub load_delay_slot: (u8, u32),
}
