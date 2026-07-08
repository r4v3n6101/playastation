use alloc::vec::Vec;

use crate::{
    cpu::{Cpu, Exception, Instruction},
    interconnect::Bus,
};

/// Fetch single instruction by `cpu.pc` and decode it
pub fn fetch_and_decode_single(cpu: &mut Cpu, bus: &mut Bus) -> Result<Instruction, Exception> {
    fetch_and_decode(cpu, bus, cpu.pc)
}

/// Fetch block of instructions and decode them.
/// It may be size of `limit`, but usually it's contiguous block until control flow change like branch or jump.
pub fn fetch_and_decode_block(
    limit: usize,
    cpu: &mut Cpu,
    bus: &mut Bus,
) -> Vec<Result<Instruction, Exception>> {
    let mut pc = cpu.pc;
    let mut pending_delay_slot = cpu.pending_jump.is_some();

    let mut output = Vec::new();
    for _ in 0..limit {
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

        pc = pc.wrapping_add(4);
    }

    output
}

fn fetch_and_decode(cpu: &mut Cpu, bus: &mut Bus, pc: u32) -> Result<Instruction, Exception> {
    let ins = match cpu.read_bus(bus, pc) {
        Ok(word) => u32::from_le_bytes(word),
        Err(exception) => {
            let exception = match exception {
                exc @ Exception::UnalignedLoad { .. } => exc,
                _ => Exception::InstructionBus { bad_vaddr: pc },
            };

            tracing::warn!(
                ?exception,
                pc=%format_args!("{pc:#X}"),
                "ins fetch failed"
            );

            return Err(exception);
        }
    };

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
