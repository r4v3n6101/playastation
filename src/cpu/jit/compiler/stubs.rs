use crate::interconnect::{Bus, BusError, BusErrorKind};

use super::super::{super::Cpu, CpuAdditionalCtx, ExecutionResult, FuncResult};

pub extern "C" fn bus_store(
    res: *mut FuncResult,
    cpu: *mut Cpu,
    bus: *mut Bus,
    addr: u32,
    val: u32,
    size: u8,
    dir: u8,
) -> i32 {
    // Safety: ptr-s are valid, since passed from compiled code.
    let res = unsafe { &mut *res };
    let cpu = unsafe { &mut *cpu };
    let bus = unsafe { &mut *bus };

    // Cache detached from memory
    if cpu.cop0.status().isc() {
        res.result = ExecutionResult::Success;
        res.bad_vaddr = 0;
        return 0;
    }

    // TODO : swl, swr
    match match size {
        1 => bus.store(addr, (val as u8).to_le_bytes()),
        2 => bus.store(addr, (val as u16).to_le_bytes()),
        4 => bus.store(addr, val.to_le_bytes()),
        _ => unreachable!(),
    } {
        Ok(()) => {
            res.result = ExecutionResult::Success;
            res.bad_vaddr = 0;
            0
        }
        Err(BusError {
            bad_vaddr,
            kind: BusErrorKind::UnalignedAddr,
        }) => {
            res.result = ExecutionResult::UnalignedStore;
            res.bad_vaddr = bad_vaddr;
            -1
        }
        Err(BusError { bad_vaddr, .. }) => {
            res.result = ExecutionResult::DataBus;
            res.bad_vaddr = bad_vaddr;
            -2
        }
    }
}

// TODO : comment
pub extern "C" fn bus_load(
    res: *mut FuncResult,
    ctx: *mut CpuAdditionalCtx,
    bus: *mut Bus,
    dest: u8,
    addr: u32,
    size: u8,
) -> i32 {
    // Safety: ptr-s are valid, since passed from compiled code.
    let res = unsafe { &mut *res };
    let ctx = unsafe { &mut *ctx };
    let bus = unsafe { &mut *bus };

    match match size {
        1 => bus.load(addr).map(u8::from_le_bytes).map(u32::from),
        2 => bus.load(addr).map(u16::from_le_bytes).map(u32::from),
        4 => bus.load(addr).map(u32::from_le_bytes),
        _ => unreachable!(),
    } {
        Ok(read) => {
            ctx.load_delay_slot = (dest, read);
            res.result = ExecutionResult::Success;
            res.bad_vaddr = 0;
            0
        }
        Err(BusError {
            bad_vaddr,
            kind: BusErrorKind::UnalignedAddr,
        }) => {
            res.result = ExecutionResult::UnalignedLoad;
            res.bad_vaddr = bad_vaddr;
            -1
        }
        Err(BusError { bad_vaddr, .. }) => {
            res.result = ExecutionResult::DataBus;
            res.bad_vaddr = bad_vaddr;
            -2
        }
    }
}
