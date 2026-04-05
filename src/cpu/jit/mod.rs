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
}

#[repr(u32)]
pub enum ExecutionResult {
    Success = 0,
    Overflow = 1,
}

/// Specific to JIT module context for CPU.
#[repr(C, packed)]
pub struct CpuAdditionalCtx {
    load_delay: bool,
}
