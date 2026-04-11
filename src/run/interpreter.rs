use std::mem;

use crate::{
    cpu::{Cpu, Exception, Opcode, PendingJump, PendingLoad},
    interconnect::{Bus, BusError, BusErrorKind},
};

use super::{ExecutionResult, Executor, decoder::Operation};

#[derive(Debug, Default)]
pub struct Interpreter;

impl Executor for Interpreter {
    fn run(&mut self, ins_block: &[Operation], cpu: &mut Cpu, bus: &mut Bus) -> ExecutionResult {
        let mut result = ExecutionResult {
            last_pc: cpu.pc,
            // Branch delay is cancelled (exception) or handled in other block
            last_in_delay_slot: false,
            exception: None,
        };
        for ins in ins_block {
            match *ins {
                Operation::Instruction {
                    pc,
                    in_delay_slot,
                    ins,
                    op,
                } => {
                    result.last_pc = pc;
                    result.last_in_delay_slot = in_delay_slot;
                    if let Err(exception) = execute(pc, ins, op, cpu, bus) {
                        result.exception.replace(exception);
                        break;
                    }
                }
                Operation::Break {
                    pc,
                    in_delay_slot,
                    cause: exception,
                } => {
                    result.last_pc = pc;
                    result.last_in_delay_slot = in_delay_slot;
                    result.exception.replace(exception);
                    break;
                }
            }
        }

        result
    }
}

fn execute(pc: u32, ins: u32, op: Opcode, cpu: &mut Cpu, bus: &mut Bus) -> Result<(), Exception> {
    let load_delay_slot = mem::take(&mut cpu.pending_load);

    let rs = ((ins >> 21) & 0x1F) as usize;
    let rt = ((ins >> 16) & 0x1F) as usize;
    let rd = ((ins >> 11) & 0x1F) as usize;
    let shamt = (ins >> 6) & 0x1F;
    let imm = ins & 0xFFFF;
    let imm_sext = i32::from((imm as u16).cast_signed());
    let target = ins & 0x03FF_FFFF;

    match op {
        // ALU ops
        Opcode::Add => {
            cpu.gpr[rd] = cpu.gpr[rs]
                .cast_signed()
                .checked_add(cpu.gpr[rt].cast_signed())
                .map(i32::cast_unsigned)
                .ok_or(Exception::Overflow)?;
        }
        Opcode::Addu => {
            cpu.gpr[rd] = cpu.gpr[rs].wrapping_add(cpu.gpr[rt]);
        }
        Opcode::Addi => {
            cpu.gpr[rt] = cpu.gpr[rs]
                .cast_signed()
                .checked_add(imm_sext)
                .map(i32::cast_unsigned)
                .ok_or(Exception::Overflow)?;
        }
        Opcode::Addiu => {
            cpu.gpr[rt] = cpu.gpr[rs].wrapping_add_signed(imm_sext);
        }
        Opcode::Sub => {
            cpu.gpr[rd] = cpu.gpr[rs]
                .cast_signed()
                .checked_sub(cpu.gpr[rt].cast_signed())
                .map(i32::cast_unsigned)
                .ok_or(Exception::Overflow)?;
        }
        Opcode::Subu => {
            cpu.gpr[rd] = cpu.gpr[rs].wrapping_sub(cpu.gpr[rt]);
        }
        Opcode::And => {
            cpu.gpr[rd] = cpu.gpr[rs] & cpu.gpr[rt];
        }
        Opcode::Or => {
            cpu.gpr[rd] = cpu.gpr[rs] | cpu.gpr[rt];
        }
        Opcode::Xor => {
            cpu.gpr[rd] = cpu.gpr[rs] ^ cpu.gpr[rt];
        }
        Opcode::Nor => {
            cpu.gpr[rd] = !(cpu.gpr[rs] | cpu.gpr[rt]);
        }
        Opcode::Slt => {
            cpu.gpr[rd] = u32::from(cpu.gpr[rs].cast_signed() < cpu.gpr[rt].cast_signed());
        }
        Opcode::Sltu => {
            cpu.gpr[rd] = u32::from(cpu.gpr[rs] < cpu.gpr[rt]);
        }
        Opcode::Sll => {
            cpu.gpr[rd] = cpu.gpr[rt].wrapping_shl(shamt);
        }
        Opcode::Srl => {
            cpu.gpr[rd] = cpu.gpr[rt].wrapping_shr(shamt);
        }
        Opcode::Sra => {
            cpu.gpr[rd] = cpu.gpr[rt]
                .cast_signed()
                .wrapping_shr(shamt)
                .cast_unsigned();
        }
        Opcode::Sllv => {
            cpu.gpr[rd] = cpu.gpr[rt].wrapping_shl(cpu.gpr[rs] & 0x1F);
        }
        Opcode::Srlv => {
            cpu.gpr[rd] = cpu.gpr[rt].wrapping_shr(cpu.gpr[rs] & 0x1F);
        }
        Opcode::Srav => {
            cpu.gpr[rd] = cpu.gpr[rt]
                .cast_signed()
                .wrapping_shr(cpu.gpr[rs] & 0x1F)
                .cast_unsigned();
        }
        Opcode::Slti => {
            cpu.gpr[rt] = u32::from(cpu.gpr[rs].cast_signed() < imm_sext);
        }
        Opcode::Sltiu => {
            cpu.gpr[rt] = u32::from(cpu.gpr[rs] < imm_sext.cast_unsigned());
        }
        Opcode::Andi => {
            cpu.gpr[rt] = cpu.gpr[rs] & imm;
        }
        Opcode::Ori => {
            cpu.gpr[rt] = cpu.gpr[rs] | imm;
        }
        Opcode::Xori => {
            cpu.gpr[rt] = cpu.gpr[rs] ^ imm;
        }
        Opcode::Lui => {
            cpu.gpr[rt] = imm << 16;
        }

        // Loads
        Opcode::Lw => {
            cpu.pending_load = PendingLoad {
                dest: rt,
                value: bus
                    .load(cpu.gpr[rs].wrapping_add_signed(imm_sext))
                    .map(u32::from_le_bytes)
                    .map_err(|err| match err {
                        BusError {
                            kind: BusErrorKind::UnalignedAddr,
                            bad_vaddr,
                        } => Exception::UnalignedLoad { bad_vaddr },
                        BusError { bad_vaddr, .. } => Exception::DataBus { bad_vaddr },
                    })?,
            };
        }
        Opcode::Lh => {
            cpu.pending_load = PendingLoad {
                dest: rt,
                value: bus
                    .load(cpu.gpr[rs].wrapping_add_signed(imm_sext))
                    .map(i16::from_le_bytes)
                    .map(i16::cast_unsigned)
                    .map(u32::from)
                    .map_err(|err| match err {
                        BusError {
                            kind: BusErrorKind::UnalignedAddr,
                            bad_vaddr,
                        } => Exception::UnalignedLoad { bad_vaddr },
                        BusError { bad_vaddr, .. } => Exception::DataBus { bad_vaddr },
                    })?,
            };
        }
        Opcode::Lhu => {
            cpu.pending_load = PendingLoad {
                dest: rt,
                value: bus
                    .load(cpu.gpr[rs].wrapping_add_signed(imm_sext))
                    .map(u16::from_le_bytes)
                    .map(u32::from)
                    .map_err(|err| match err {
                        BusError {
                            kind: BusErrorKind::UnalignedAddr,
                            bad_vaddr,
                        } => Exception::UnalignedLoad { bad_vaddr },
                        BusError { bad_vaddr, .. } => Exception::DataBus { bad_vaddr },
                    })?,
            };
        }
        Opcode::Lb => {
            cpu.pending_load = PendingLoad {
                dest: rt,
                value: bus
                    .load(cpu.gpr[rs].wrapping_add_signed(imm_sext))
                    .map(i8::from_le_bytes)
                    .map(i8::cast_unsigned)
                    .map(u32::from)
                    .map_err(|err| match err {
                        BusError {
                            kind: BusErrorKind::UnalignedAddr,
                            bad_vaddr,
                        } => Exception::UnalignedLoad { bad_vaddr },
                        BusError { bad_vaddr, .. } => Exception::DataBus { bad_vaddr },
                    })?,
            };
        }
        Opcode::Lbu => {
            cpu.pending_load = PendingLoad {
                dest: rt,
                value: bus
                    .load(cpu.gpr[rs].wrapping_add_signed(imm_sext))
                    .map(u8::from_le_bytes)
                    .map(u32::from)
                    .map_err(|err| match err {
                        BusError {
                            kind: BusErrorKind::UnalignedAddr,
                            bad_vaddr,
                        } => Exception::UnalignedLoad { bad_vaddr },
                        BusError { bad_vaddr, .. } => Exception::DataBus { bad_vaddr },
                    })?,
            };
        }
        Opcode::Lwl => {
            todo!()
        }
        Opcode::Lwr => {
            todo!()
        }

        // Ignore writes if IsC=1
        Opcode::Sw | Opcode::Sh | Opcode::Sb | Opcode::Swl | Opcode::Swr
            if cpu.cop0.status().isc() => {}

        // Stores
        Opcode::Sw => {
            bus.store(
                cpu.gpr[rs].wrapping_add_signed(imm_sext),
                cpu.gpr[rt].to_le_bytes(),
            )
            .map_err(|err| match err {
                BusError {
                    kind: BusErrorKind::UnalignedAddr,
                    bad_vaddr,
                } => Exception::UnalignedStore { bad_vaddr },
                BusError { bad_vaddr, .. } => Exception::DataBus { bad_vaddr },
            })?;
        }
        Opcode::Sh => {
            bus.store(
                cpu.gpr[rs].wrapping_add_signed(imm_sext),
                (cpu.gpr[rt] as u16).to_le_bytes(),
            )
            .map_err(|err| match err {
                BusError {
                    kind: BusErrorKind::UnalignedAddr,
                    bad_vaddr,
                } => Exception::UnalignedStore { bad_vaddr },
                BusError { bad_vaddr, .. } => Exception::DataBus { bad_vaddr },
            })?;
        }
        Opcode::Sb => {
            bus.store(
                cpu.gpr[rs].wrapping_add_signed(imm_sext),
                (cpu.gpr[rt] as u8).to_le_bytes(),
            )
            .map_err(|err| match err {
                BusError {
                    kind: BusErrorKind::UnalignedAddr,
                    bad_vaddr,
                } => Exception::UnalignedStore { bad_vaddr },
                BusError { bad_vaddr, .. } => Exception::DataBus { bad_vaddr },
            })?;
        }
        Opcode::Swl => {
            todo!()
        }
        Opcode::Swr => {
            todo!()
        }

        // Branches
        Opcode::Beq => {
            cpu.pending_jump = PendingJump {
                happen: cpu.gpr[rs] == cpu.gpr[rt],
                target: pc.wrapping_add(4).wrapping_add_signed(imm_sext << 2),
            };
        }
        Opcode::Bne => {
            cpu.pending_jump = PendingJump {
                happen: cpu.gpr[rs] != cpu.gpr[rt],
                target: pc.wrapping_add(4).wrapping_add_signed(imm_sext << 2),
            };
        }
        Opcode::Bgez => {
            cpu.pending_jump = PendingJump {
                happen: cpu.gpr[rs].cast_signed() >= 0,
                target: pc.wrapping_add(4).wrapping_add_signed(imm_sext << 2),
            };
        }
        Opcode::Blez => {
            cpu.pending_jump = PendingJump {
                happen: cpu.gpr[rs].cast_signed() <= 0,
                target: pc.wrapping_add(4).wrapping_add_signed(imm_sext << 2),
            };
        }
        Opcode::Bgtz => {
            cpu.pending_jump = PendingJump {
                happen: cpu.gpr[rs].cast_signed() > 0,
                target: pc.wrapping_add(4).wrapping_add_signed(imm_sext << 2),
            };
        }
        Opcode::Bltz => {
            cpu.pending_jump = PendingJump {
                happen: cpu.gpr[rs].cast_signed() < 0,
                target: pc.wrapping_add(4).wrapping_add_signed(imm_sext << 2),
            };
        }
        Opcode::Bgezal => {
            cpu.gpr[Cpu::DEFAULT_LINK_REG] = pc + 8;

            cpu.pending_jump = PendingJump {
                happen: cpu.gpr[rs].cast_signed() >= 0,
                target: pc.wrapping_add(4).wrapping_add_signed(imm_sext << 2),
            };
        }
        Opcode::Bltzal => {
            cpu.gpr[Cpu::DEFAULT_LINK_REG] = pc + 8;

            cpu.pending_jump = PendingJump {
                happen: cpu.gpr[rs].cast_signed() < 0,
                target: pc.wrapping_add(4).wrapping_add_signed(imm_sext << 2),
            };
        }

        // Jumps
        Opcode::J => {
            cpu.pending_jump = PendingJump {
                happen: true,
                target: (pc.wrapping_add(4) & 0xF000_0000) | (target << 2),
            };
        }
        Opcode::Jal => {
            cpu.gpr[Cpu::DEFAULT_LINK_REG] = pc.wrapping_add(8);

            cpu.pending_jump = PendingJump {
                happen: true,
                target: (pc.wrapping_add(4) & 0xF000_0000) | (target << 2),
            };
        }
        Opcode::Jr => {
            cpu.pending_jump = PendingJump {
                happen: true,
                target: cpu.gpr[rs],
            };
        }
        Opcode::Jalr => {
            cpu.gpr[rd] = pc.wrapping_add(8);
            cpu.pending_jump = PendingJump {
                happen: true,
                target: cpu.gpr[rs],
            };
        }

        // MulDiv
        Opcode::Mult => {
            let a = i64::from(cpu.gpr[rs].cast_signed());
            let b = i64::from(cpu.gpr[rt].cast_signed());
            let res = (a * b).cast_unsigned();

            cpu.hi = (res >> 32) as u32;
            cpu.lo = res as u32;
        }
        Opcode::Multu => {
            let a = u64::from(cpu.gpr[rs]);
            let b = u64::from(cpu.gpr[rt]);
            let res = a * b;

            cpu.hi = (res >> 32) as u32;
            cpu.lo = res as u32;
        }
        Opcode::Div => {
            let a = cpu.gpr[rs].cast_signed();
            let b = cpu.gpr[rt].cast_signed();

            // Overflow or div by 0
            let (hi, lo) = if (b == 0) || (a.cast_unsigned() == 0x8000_0000 && b == -1) {
                (a.cast_unsigned(), b.cast_unsigned())
            } else {
                ((a % b).cast_unsigned(), (a / b).cast_unsigned())
            };

            cpu.hi = hi;
            cpu.lo = lo;
        }
        Opcode::Divu => {
            let a = cpu.gpr[rs];
            let b = cpu.gpr[rt];
            let (hi, lo) = if b == 0 { (a, b) } else { (a % b, a / b) };

            cpu.hi = hi;
            cpu.lo = lo;
        }

        // From/to copies
        Opcode::Mfhi => {
            cpu.gpr[rd] = cpu.hi;
        }
        Opcode::Mflo => {
            cpu.gpr[rd] = cpu.lo;
        }
        Opcode::Mtlo => {
            cpu.lo = cpu.gpr[rs];
        }
        Opcode::Mthi => {
            cpu.hi = cpu.gpr[rs];
        }
        Opcode::Mfc0 => {
            cpu.pending_load = PendingLoad {
                dest: rt,
                value: cpu.cop0.regs[rd],
            };
        }
        Opcode::Mtc0 => {
            cpu.cop0.regs[rd] = cpu.gpr[rt];
        }
        Opcode::Cfc0 => unimplemented!(),
        Opcode::Ctc0 => unimplemented!(),

        // Return state before exception
        Opcode::Rfe => {
            cpu.cop0.exception_leave();
        }

        // Exceptions
        Opcode::Break => return Err(Exception::Break),
        Opcode::Syscall => return Err(Exception::Syscall),
    }

    cpu.gpr[load_delay_slot.dest] = load_delay_slot.value;
    cpu.gpr[0] = 0;

    Ok(())
}
