use crate::interconnect::{Bus, BusError, BusErrorKind};

use super::super::{super::Cpu, ExecutionResult, FuncResult};

pub extern "C" fn bus_load(
    res: *mut FuncResult,
    bus: *mut Bus,
    load_delay_reg: *mut u8,
    load_delay_val: *mut u32,
    dest: u8,
    addr: u32,
    size: u8,
    signed: u8,
    // 0 is for usual ops, 1 - left, 2 - right
    dir: u8,
) -> i8 {
    // Safety: ptr-s are valid, since passed from compiled code.
    let res = unsafe { &mut *res };
    let bus = unsafe { &mut *bus };
    let load_delay_reg = unsafe { &mut *load_delay_reg };
    let load_delay_val = unsafe { &mut *load_delay_val };

    res.loads += 1;
    match match size {
        1 => bus.load(addr).map(|x| {
            if signed == 1 {
                i8::from_le_bytes(x) as u32
            } else {
                u32::from(u8::from_be_bytes(x))
            }
        }),
        2 => bus.load(addr).map(|x| {
            if signed == 1 {
                i16::from_le_bytes(x) as u32
            } else {
                u32::from(u16::from_be_bytes(x))
            }
        }),
        4 => bus.load(addr).map(|x| {
            if signed == 1 {
                i32::from_le_bytes(x) as u32
            } else {
                u32::from_be_bytes(x)
            }
        }),
        _ => unreachable!(),
    } {
        Ok(read) => {
            *load_delay_reg = dest;
            *load_delay_val = read;

            // RAM read latency
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

pub extern "C" fn bus_store(
    res: *mut FuncResult,
    cpu: *mut Cpu,
    bus: *mut Bus,
    addr: u32,
    val: u32,
    size: u8,
    // 0 is for usual ops, 1 - left, 2 - right
    dir: u8,
) -> i8 {
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
