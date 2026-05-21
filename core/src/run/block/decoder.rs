use alloc::vec::Vec;

use crate::{
    cpu::{Cpu, Exception, Opcode},
    interconnect::Bus,
};

use super::Operation;

/// Fetch instructions and decode them.
/// Size is limited to `limit`, but may be `limit + 1` in case of `limit` element is branch/jump.
pub fn fetch_and_decode_block(mut limit: usize, cpu: &mut Cpu, bus: &mut Bus) -> Vec<Operation> {
    let mut pc = cpu.pc;
    let mut pending_delay_slot = false;

    let mut output = Vec::new();
    loop {
        if limit == 0 && !pending_delay_slot {
            break;
        }

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
                output.push(Operation::Break {
                    pc,
                    in_delay_slot: pending_delay_slot,
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
            output.push(Operation::Break {
                pc,
                in_delay_slot: pending_delay_slot,
                cause: Exception::ReservedInstruction,
            });
            break;
        };

        match op {
            Opcode::Syscall => {
                output.push(Operation::Break {
                    pc,
                    in_delay_slot: pending_delay_slot,
                    cause: Exception::Syscall,
                });
                break;
            }
            Opcode::Break => {
                output.push(Operation::Break {
                    pc,
                    in_delay_slot: pending_delay_slot,
                    cause: Exception::Break,
                });
                break;
            }
            _ => {
                output.push(Operation::Instruction {
                    pc,
                    in_delay_slot: pending_delay_slot,
                    ins,
                    op,
                });
            }
        }

        if pending_delay_slot {
            break;
        }

        if op.has_branch_delay() {
            pending_delay_slot = true;
        }

        pc = pc.wrapping_add(4);
        limit -= 1;
    }

    output
}

#[cfg(test)]
mod tests {
    use crate::cpu::Cpu;

    use super::*;

    fn make_bus(words: &[(u32, u32)]) -> Bus {
        let mut bus = Bus::default();

        words.iter().for_each(|&(addr, val)| {
            bus.store(addr, val.to_le_bytes());
        });

        bus
    }

    #[test]
    fn stops_at_branch_after_delay_slot() {
        // beq $zero, $zero, +1
        // nop
        // nop   <- must NOT be included
        let mut cpu = Cpu {
            pc: 0,
            ..Default::default()
        };
        let mut bus = make_bus(&[
            (0x0000_0000, 0x1000_0001),
            (0x0000_0004, 0x0000_0000),
            (0x0000_0008, 0x0000_0000),
        ]);

        let out = fetch_and_decode_block(1024, &mut cpu, &mut bus);

        assert_eq!(out.len(), 2);

        match out[0] {
            Operation::Instruction {
                pc: 0x0000_0000,
                ins: 0x1000_0001,
                op,
                ..
            } if op.has_branch_delay() => {}
            _ => panic!("expected decoded branch"),
        }

        match out[1] {
            Operation::Instruction {
                pc: 0x0000_0004,
                in_delay_slot: true,
                ins: 0x0000_0000,
                ..
            } => {}
            _ => panic!("expected decoded delay slot"),
        }
    }

    #[test]
    fn stops_at_jr_after_delay_slot() {
        // jr $ra
        // nop
        // nop   <- must NOT be included
        let mut cpu = Cpu {
            pc: 0,
            ..Default::default()
        };
        let mut bus = make_bus(&[
            (0x0000_0000, 0x03E0_0008),
            (0x0000_0004, 0x0000_0000),
            (0x0000_0008, 0x0000_0000),
        ]);

        let out = fetch_and_decode_block(1024, &mut cpu, &mut bus);

        assert_eq!(out.len(), 2);

        match out[0] {
            Operation::Instruction { pc: 0, op, .. } if op.has_branch_delay() => {}
            _ => panic!("expected decoded jr"),
        }

        match out[1] {
            Operation::Instruction {
                pc: 4,
                in_delay_slot: true,
                ..
            } => {}
            _ => panic!("expected decoded delay slot"),
        }
    }

    #[test]
    fn stops_immediately_on_syscall() {
        // syscall
        // nop   <- must NOT be included
        let mut cpu = Cpu {
            pc: 0,
            ..Default::default()
        };
        let mut bus = make_bus(&[(0x0000_0000, 0x0000_000C), (0x0000_0004, 0x0000_0000)]);

        let out = fetch_and_decode_block(1024, &mut cpu, &mut bus);

        assert_eq!(out.len(), 1);

        match out[0] {
            Operation::Break {
                pc: 0,
                cause: Exception::Syscall,
                ..
            } => {}
            _ => panic!("expected decoded syscall"),
        }
    }

    #[test]
    fn stops_immediately_on_break() {
        // break
        // nop   <- must NOT be included
        let mut cpu = Cpu {
            pc: 0,
            ..Default::default()
        };
        let mut bus = make_bus(&[(0x0000_0000, 0x0000_000D), (0x0000_0004, 0x0000_0000)]);

        let out = fetch_and_decode_block(1024, &mut cpu, &mut bus);

        assert_eq!(out.len(), 1);

        match out[0] {
            Operation::Break {
                pc: 0,
                cause: Exception::Break,
                ..
            } => {}
            _ => panic!("expected decoded break"),
        }
    }

    #[test]
    fn returns_reserved_instruction_exception_and_stops() {
        // 0xFFFF_FFFF should not decode on MIPS
        let mut cpu = Cpu {
            pc: 0,
            ..Default::default()
        };
        let mut bus = make_bus(&[(0x0000_0000, 0xFFFF_FFFF), (0x0000_0004, 0x0000_0000)]);

        let out = fetch_and_decode_block(1024, &mut cpu, &mut bus);

        assert_eq!(out.len(), 1);
        assert!(matches!(
            out[0],
            Operation::Break {
                cause: Exception::ReservedInstruction,
                ..
            }
        ));
    }
}
