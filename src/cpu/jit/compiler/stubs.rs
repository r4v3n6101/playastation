use crate::{
    cpu::Cpu,
    interconnect::{Bus, BusError, BusErrorKind},
};

use super::super::{ExecutionResult, FuncResult};

pub extern "C" fn bus_load<const SIZE: usize, const SIGNED: bool>(
    res: *mut FuncResult,
    cpu: *mut Cpu,
    bus: *mut Bus,
    load_delay_dest: *mut u8,
    load_delay_val: *mut u32,
    dest: u8,
    addr: u32,
) -> i8 {
    // Safety: ptr-s are valid, since passed from compiled code.
    let res = unsafe { &mut *res };
    let cpu = unsafe { &mut *cpu };
    let bus = unsafe { &mut *bus };
    let load_delay_dest = unsafe { &mut *load_delay_dest };
    let load_delay_val = unsafe { &mut *load_delay_val };

    // Cache detached from memory
    if cpu.cop0.status().isc() {
        res.result = ExecutionResult::Success;
        return 0;
    }

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
            *load_delay_dest = dest;
            *load_delay_val = read;

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
        res.result = ExecutionResult::Success;
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
