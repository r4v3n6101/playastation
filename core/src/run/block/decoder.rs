use alloc::vec::Vec;

use crate::{
    cpu::{Cpu, Exception, Opcode},
    interconnect::Bus,
};

use super::Operation;

/// Fetch instructions and decode them
pub fn fetch_and_decode_block(limit: usize, cpu: &mut Cpu, bus: &mut Bus) -> Vec<Operation> {
    let mut pc = cpu.pc;
    let mut pending_delay_slot = cpu.pending_jump.valid;

    let mut output = Vec::new();
    for _ in 0..limit {
        let ins = match cpu.read_bus(bus, pc) {
            Ok(word) => u32::from_le_bytes(word),
            Err(exception) => {
                let exception = match exception {
                    Exception::DataBus { bad_vaddr } => Exception::InstructionBus { bad_vaddr },
                    other => other,
                };

                tracing::warn!(
                    ?exception,
                    pc=%format_args!("{pc:#X}"),
                    "ins fetch failed"
                );
                output.push(Operation::Error {
                    pc,
                    cause: exception,
                });
                break;
            }
        };

        let Some(op) = Opcode::decode(ins) else {
            tracing::warn!(
                pc=%format_args!("{pc:#X}"),
                ins=%format_args!("{ins:#X}"),
                "ins decode failed"
            );
            output.push(Operation::Error {
                pc,
                cause: Exception::ReservedInstruction,
            });
            break;
        };

        output.push(Operation::Instruction { pc, ins, op });

        if let Opcode::Syscall | Opcode::Break = op {
            break;
        }

        if pending_delay_slot {
            break;
        }

        if op.has_branch_delay() {
            pending_delay_slot = true;
        }

        pc = pc.wrapping_add(4);
    }

    output
}
