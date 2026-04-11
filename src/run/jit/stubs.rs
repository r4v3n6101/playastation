use crate::{
    cpu::{Cpu, PendingLoad},
    interconnect::{Bus, BusError, BusErrorKind},
};

use super::{ExecutionResult, FuncResult};

pub extern "C" fn bus_load<const SIZE: usize, const SIGNED: bool>(
    res: *mut FuncResult,
    cpu: *mut Cpu,
    bus: *mut Bus,
    dest: usize,
    addr: u32,
) -> i8 {
    // Safety: ptr-s are valid, since passed from compiled code.
    let res = unsafe { &mut *res };
    let cpu = unsafe { &mut *cpu };
    let bus = unsafe { &mut *bus };

    match match SIZE {
        1 => bus.load(addr).map(|x| {
            if SIGNED {
                i32::from(i8::from_le_bytes(x)).cast_unsigned()
            } else {
                u32::from(u8::from_le_bytes(x))
            }
        }),
        2 => bus.load(addr).map(|x| {
            if SIGNED {
                i32::from(i16::from_le_bytes(x)).cast_unsigned()
            } else {
                u32::from(u16::from_le_bytes(x))
            }
        }),
        4 => bus.load(addr).map(u32::from_le_bytes),
        _ => unreachable!(),
    } {
        Ok(read) => {
            cpu.pending_load = PendingLoad { dest, value: read };

            res.result = ExecutionResult::Success;
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

pub extern "C" fn bus_store<const SIZE: usize>(
    res: *mut FuncResult,
    cpu: *mut Cpu,
    bus: *mut Bus,
    addr: u32,
    val: u32,
) -> i8 {
    // Safety: ptr-s are valid, since passed from compiled code.
    let res = unsafe { &mut *res };
    let cpu = unsafe { &mut *cpu };
    let bus = unsafe { &mut *bus };

    // Cache detached from memory
    if cpu.cop0.status().isc() {
        return 0;
    }

    // TODO : swl, swr
    match match SIZE {
        1 => bus.store(addr, (val as u8).to_le_bytes()),
        2 => bus.store(addr, (val as u16).to_le_bytes()),
        4 => bus.store(addr, val.to_le_bytes()),
        _ => unreachable!(),
    } {
        Ok(()) => {
            res.result = ExecutionResult::Success;
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

pub extern "C" fn rfe(cpu: *mut Cpu) {
    // Safety: ptr-s are valid, since passed from compiled code.
    let cpu = unsafe { &mut *cpu };

    cpu.cop0.exception_leave();
}
