use crate::interconnect::Bus;

use super::Cpu;

mod compiler;
mod decoder;

pub type FuncPtr = fn(*mut FuncResult, *mut Cpu, *mut Bus);

#[repr(C)]
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct FuncResult {
    /// Result of function execution.
    result: ExecutionResult,
    /// PC of last executed instruction.
    last_pc: u32,
    /// Flag (0=false, 1=true) whether last executed instruction is in delay slot.
    last_in_delay_slot: u32,

    // Ideally this should be inside [`ExecutionResult`]
    /// Filled in case of invalid memory ops (unaligned load/store, unmapped).
    bad_vaddr: u32,
    /// PC being jumped to.
    jump_addr: u32,
}

#[repr(u32)]
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub enum ExecutionResult {
    #[default]
    Success = 0,
    Jump = 1,
    Overflow = 2,
    UnalignedLoad = 3,
    UnalignedStore = 4,
    ReservedInstruction = 5,
    InstructionBus = 6,
    DataBus = 7,
    Syscall = 8,
    Break = 9,
}
