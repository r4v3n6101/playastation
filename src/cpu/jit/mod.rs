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
    /// Filled in case of invalid memory ops (unaligned load/store, unmapped)
    bad_vaddr: u32,
}

#[repr(u32)]
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub enum ExecutionResult {
    #[default]
    Success = 0,
    Overflow = 1,
    UnalignedLoad = 2,
    UnalignedStore = 3,
    DataBus = 4,
}
