use alloc::vec::Vec;

use crate::machine::{
    cpu::{Cpu, Exception, Instruction},
    interconnect::{RAM_SIZE, Region, bus::Bus, region_of},
};

/// Fetch single instruction by `cpu.pc` and decode it
pub fn fetch_and_decode_single(cpu: &mut Cpu, bus: &mut Bus) -> Result<Instruction, Exception> {
    fetch_and_decode(cpu, bus, cpu.pc)
}

/// Fetch block of instructions and decode them.
/// It may be size of `limit`, but usually it's contiguous block until control flow change like branch or jump.
pub fn fetch_and_decode_block(
    cpu: &mut Cpu,
    bus: &mut Bus,
    limit: usize,
) -> Vec<Result<Instruction, Exception>> {
    let start = cpu.pc;

    // RAM mirrors share blocks. Only the last mirror is followed by unmapped space.
    let last_byte = cpu.mmu.mapped_segment(start).map_or(start, |segment| {
        if region_of(start - segment.start) == Region::Ram {
            start | (RAM_SIZE as u32 - 1)
        } else {
            segment.end - 1
        }
    });
    let mut pending_delay_slot = cpu.pending_jump.is_some();

    let mut output = Vec::new();
    for pc in (start..=last_byte).step_by(Instruction::SIZE).take(limit) {
        let ins = match fetch_and_decode(cpu, bus, pc) {
            Ok(ins) => ins,
            Err(exc) => {
                output.push(Err(exc));
                break;
            }
        };

        output.push(Ok(ins));

        // It may jump to exception handler or being interrupted
        if let Instruction::Syscall { .. } | Instruction::Break { .. } | Instruction::Rfe = ins {
            break;
        }

        if pending_delay_slot {
            break;
        }

        if ins.has_branch_delay() {
            pending_delay_slot = true;
        }
    }

    output
}

fn fetch_and_decode(cpu: &mut Cpu, bus: &mut Bus, pc: u32) -> Result<Instruction, Exception> {
    let ins = cpu.read_pc(bus, pc).inspect_err(|exception| {
        tracing::warn!(
            ?exception,
            pc=%format_args!("{pc:#X}"),
            "ins fetch failed"
        );
    })?;

    let Some(ins) = Instruction::decode(ins) else {
        tracing::warn!(
            pc=%format_args!("{pc:#X}"),
            ins=%format_args!("{ins:#X}"),
            "ins decode failed"
        );

        return Err(Exception::ReservedInstruction);
    };

    Ok(ins)
}
