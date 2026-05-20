use alloc::rc::Rc;

use crate::{
    cpu::{Cpu, Exception, Opcode, PendingLoad, TranslationResult},
    interconnect::Bus,
};

use super::{
    ExecutionResult,
    block::{Block, Operation, PagedCache},
};

/// How many cycles need to be elapsed, so hi/lo become available after Mul op.
const MULT_HI_LO_LOAD_LATENCY: u64 = 5;
/// Same as above, but for Div ops.
const DIV_HI_LO_LOAD_LATENCY: u64 = 35;

struct Context<'a> {
    result: ExecutionResult,
    hi_lo_latency: u64,

    blk_cache: &'a mut PagedCache,
    block: Rc<Block>,
}

enum StopReason {
    Exception(Exception),
    BlockInvalidated,
}

pub fn run(
    blk_cache: &mut PagedCache,
    block: Rc<Block>,
    cpu: &mut Cpu,
    bus: &mut Bus,
) -> ExecutionResult {
    let mut ctx = Context {
        result: ExecutionResult {
            last_pc: cpu.pc,
            // Branch delay is cancelled (exception) or handled in other block
            last_in_delay_slot: false,
            jump: false,
            jump_target: 0,
            cycles_elapsed: 0,
            exception: None,
        },
        hi_lo_latency: 0,

        blk_cache,
        block: Rc::clone(&block),
    };

    for ins in &block.ops {
        match *ins {
            Operation::Instruction {
                pc,
                in_delay_slot,
                ins,
                op,
            } => {
                ctx.result.last_pc = pc;
                ctx.result.last_in_delay_slot = in_delay_slot;

                let res = execute(&mut ctx, ins, op, cpu, bus);
                ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(1);
                ctx.hi_lo_latency = ctx.hi_lo_latency.saturating_sub(1);

                match res {
                    Err(StopReason::Exception(exc)) => {
                        ctx.result.exception.replace(exc);
                        break;
                    }
                    Err(StopReason::BlockInvalidated) => {
                        break;
                    }
                    _ => {}
                }
            }
            Operation::Break {
                pc,
                in_delay_slot,
                cause,
            } => {
                ctx.result.last_pc = pc;
                ctx.result.last_in_delay_slot = in_delay_slot;
                ctx.result.exception.replace(cause);

                // Cycles
                ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(1);
                ctx.hi_lo_latency = ctx.hi_lo_latency.saturating_sub(1);

                break;
            }
        }
    }

    // The next block won't wait latency before HI/LO, because we emulate it in the current one.
    ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(ctx.hi_lo_latency);

    ctx.result
}

fn execute(
    ctx: &mut Context,
    ins: u32,
    op: Opcode,
    cpu: &mut Cpu,
    bus: &mut Bus,
) -> Result<(), StopReason> {
    let rs = ((ins >> 21) & 0x1F) as usize;
    let rt = ((ins >> 16) & 0x1F) as usize;
    let rd = ((ins >> 11) & 0x1F) as usize;
    let shamt = (ins >> 6) & 0x1F;
    let imm = ins & 0xFFFF;
    let imm_sext = i32::from((imm as u16).cast_signed());
    let target = ins & 0x03FF_FFFF;

    let mut pending_load = PendingLoad::default();
    let mut written_vaddr = None;
    match op {
        // ALU ops
        Opcode::Add => {
            cpu.write_gpr(
                rd,
                cpu.gpr[rs]
                    .cast_signed()
                    .checked_add(cpu.gpr[rt].cast_signed())
                    .map(i32::cast_unsigned)
                    .ok_or(StopReason::Exception(Exception::Overflow))?,
            );
        }
        Opcode::Addu => {
            cpu.write_gpr(rd, cpu.gpr[rs].wrapping_add(cpu.gpr[rt]));
        }
        Opcode::Addi => {
            cpu.write_gpr(
                rt,
                cpu.gpr[rs]
                    .cast_signed()
                    .checked_add(imm_sext)
                    .map(i32::cast_unsigned)
                    .ok_or(StopReason::Exception(Exception::Overflow))?,
            );
        }
        Opcode::Addiu => {
            cpu.write_gpr(rt, cpu.gpr[rs].wrapping_add_signed(imm_sext));
        }
        Opcode::Sub => {
            cpu.write_gpr(
                rd,
                cpu.gpr[rs]
                    .cast_signed()
                    .checked_sub(cpu.gpr[rt].cast_signed())
                    .map(i32::cast_unsigned)
                    .ok_or(StopReason::Exception(Exception::Overflow))?,
            );
        }
        Opcode::Subu => {
            cpu.write_gpr(rd, cpu.gpr[rs].wrapping_sub(cpu.gpr[rt]));
        }
        Opcode::And => {
            cpu.write_gpr(rd, cpu.gpr[rs] & cpu.gpr[rt]);
        }
        Opcode::Or => {
            cpu.write_gpr(rd, cpu.gpr[rs] | cpu.gpr[rt]);
        }
        Opcode::Xor => {
            cpu.write_gpr(rd, cpu.gpr[rs] ^ cpu.gpr[rt]);
        }
        Opcode::Nor => {
            cpu.write_gpr(rd, !(cpu.gpr[rs] | cpu.gpr[rt]));
        }
        Opcode::Slt => {
            cpu.write_gpr(
                rd,
                u32::from(cpu.gpr[rs].cast_signed() < cpu.gpr[rt].cast_signed()),
            );
        }
        Opcode::Sltu => {
            cpu.write_gpr(rd, u32::from(cpu.gpr[rs] < cpu.gpr[rt]));
        }
        Opcode::Sll => {
            cpu.write_gpr(rd, cpu.gpr[rt].wrapping_shl(shamt));
        }
        Opcode::Srl => {
            cpu.write_gpr(rd, cpu.gpr[rt].wrapping_shr(shamt));
        }
        Opcode::Sra => {
            cpu.write_gpr(
                rd,
                cpu.gpr[rt]
                    .cast_signed()
                    .wrapping_shr(shamt)
                    .cast_unsigned(),
            );
        }
        Opcode::Sllv => {
            cpu.write_gpr(rd, cpu.gpr[rt].wrapping_shl(cpu.gpr[rs] & 0x1F));
        }
        Opcode::Srlv => {
            cpu.write_gpr(rd, cpu.gpr[rt].wrapping_shr(cpu.gpr[rs] & 0x1F));
        }
        Opcode::Srav => {
            cpu.write_gpr(
                rd,
                cpu.gpr[rt]
                    .cast_signed()
                    .wrapping_shr(cpu.gpr[rs] & 0x1F)
                    .cast_unsigned(),
            );
        }
        Opcode::Slti => {
            cpu.write_gpr(rt, u32::from(cpu.gpr[rs].cast_signed() < imm_sext));
        }
        Opcode::Sltiu => {
            cpu.write_gpr(rt, u32::from(cpu.gpr[rs] < imm_sext.cast_unsigned()));
        }
        Opcode::Andi => {
            cpu.write_gpr(rt, cpu.gpr[rs] & imm);
        }
        Opcode::Ori => {
            cpu.write_gpr(rt, cpu.gpr[rs] | imm);
        }
        Opcode::Xori => {
            cpu.write_gpr(rt, cpu.gpr[rs] ^ imm);
        }
        Opcode::Lui => {
            cpu.write_gpr(rt, imm << 16);
        }

        // Loads
        Opcode::Lw => {
            pending_load = PendingLoad {
                dest: rt,
                value: cpu
                    .read_bus(bus, cpu.gpr[rs].wrapping_add_signed(imm_sext))
                    .map(u32::from_le_bytes)
                    .map_err(StopReason::Exception)?,
            };
        }
        Opcode::Lh => {
            pending_load = PendingLoad {
                dest: rt,
                value: cpu
                    .read_bus(bus, cpu.gpr[rs].wrapping_add_signed(imm_sext))
                    .map(i16::from_le_bytes)
                    .map(i32::from)
                    .map(i32::cast_unsigned)
                    .map_err(StopReason::Exception)?,
            };
        }
        Opcode::Lhu => {
            pending_load = PendingLoad {
                dest: rt,
                value: cpu
                    .read_bus(bus, cpu.gpr[rs].wrapping_add_signed(imm_sext))
                    .map(u16::from_le_bytes)
                    .map(u32::from)
                    .map_err(StopReason::Exception)?,
            };
        }
        Opcode::Lb => {
            pending_load = PendingLoad {
                dest: rt,
                value: cpu
                    .read_bus(bus, cpu.gpr[rs].wrapping_add_signed(imm_sext))
                    .map(i8::from_le_bytes)
                    .map(i32::from)
                    .map(i32::cast_unsigned)
                    .map_err(StopReason::Exception)?,
            };
        }
        Opcode::Lbu => {
            pending_load = PendingLoad {
                dest: rt,
                value: cpu
                    .read_bus(bus, cpu.gpr[rs].wrapping_add_signed(imm_sext))
                    .map(u8::from_le_bytes)
                    .map(u32::from)
                    .map_err(StopReason::Exception)?,
            };
        }
        Opcode::Lwl => {
            let addr = cpu.gpr[rs].wrapping_add_signed(imm_sext);
            let word = cpu
                .read_bus(bus, addr & !3)
                .map(u32::from_le_bytes)
                .map_err(StopReason::Exception)?;
            let old = if rt == cpu.pending_load.dest {
                cpu.pending_load.value
            } else {
                cpu.gpr[rt]
            };

            pending_load = PendingLoad {
                dest: rt,
                value: match addr & 3 {
                    0 => (old & 0x00FF_FFFF) | (word << 24),
                    1 => (old & 0x0000_FFFF) | (word << 16),
                    2 => (old & 0x0000_00FF) | (word << 8),
                    3 => word,
                    _ => unreachable!(),
                },
            };
        }
        Opcode::Lwr => {
            let addr = cpu.gpr[rs].wrapping_add_signed(imm_sext);
            let word = cpu
                .read_bus(bus, addr & !3)
                .map(u32::from_le_bytes)
                .map_err(StopReason::Exception)?;
            let old = if rt == cpu.pending_load.dest {
                cpu.pending_load.value
            } else {
                cpu.gpr[rt]
            };

            pending_load = PendingLoad {
                dest: rt,
                value: match addr & 3 {
                    0 => word,
                    1 => (old & 0xFF00_0000) | (word >> 8),
                    2 => (old & 0xFFFF_0000) | (word >> 16),
                    3 => (old & 0xFFFF_FF00) | (word >> 24),
                    _ => unreachable!(),
                },
            };
        }

        // Ignore writes if IsC=1
        Opcode::Sw | Opcode::Sh | Opcode::Sb | Opcode::Swl | Opcode::Swr
            if cpu.cop0.status().isc() => {}

        // Stores
        Opcode::Sw => {
            let vaddr = cpu.gpr[rs].wrapping_add_signed(imm_sext);
            cpu.write_bus(bus, vaddr, cpu.gpr[rt].to_le_bytes())
                .map_err(StopReason::Exception)?;
            written_vaddr = Some(vaddr);
        }
        Opcode::Sh => {
            let vaddr = cpu.gpr[rs].wrapping_add_signed(imm_sext);
            cpu.write_bus(bus, vaddr, (cpu.gpr[rt] as u16).to_le_bytes())
                .map_err(StopReason::Exception)?;
            written_vaddr = Some(vaddr);
        }
        Opcode::Sb => {
            let vaddr = cpu.gpr[rs].wrapping_add_signed(imm_sext);
            cpu.write_bus(bus, vaddr, (cpu.gpr[rt] as u8).to_le_bytes())
                .map_err(StopReason::Exception)?;
            written_vaddr = Some(vaddr);
        }
        Opcode::Swl => {
            let vaddr = cpu.gpr[rs].wrapping_add_signed(imm_sext);
            let word = cpu
                .read_bus(bus, vaddr & !3)
                .map(u32::from_le_bytes)
                .map_err(StopReason::Exception)?;

            let val = match vaddr & 3 {
                0 => (word & 0xFFFF_FF00) | (cpu.gpr[rt] >> 24),
                1 => (word & 0xFFFF_0000) | (cpu.gpr[rt] >> 16),
                2 => (word & 0xFF00_0000) | (cpu.gpr[rt] >> 8),
                3 => cpu.gpr[rt],
                _ => unreachable!(),
            };

            cpu.write_bus(bus, vaddr & !3, val.to_le_bytes())
                .map_err(StopReason::Exception)?;
            written_vaddr = Some(vaddr & !3);
        }
        Opcode::Swr => {
            let vaddr = cpu.gpr[rs].wrapping_add_signed(imm_sext);
            let word = cpu
                .read_bus(bus, vaddr & !3)
                .map(u32::from_le_bytes)
                .map_err(StopReason::Exception)?;

            let val = match vaddr & 3 {
                0 => cpu.gpr[rt],
                1 => (word & 0x0000_00FF) | (cpu.gpr[rt] << 8),
                2 => (word & 0x0000_FFFF) | (cpu.gpr[rt] << 16),
                3 => (word & 0x00FF_FFFF) | (cpu.gpr[rt] << 24),
                _ => unreachable!(),
            };

            cpu.write_bus(bus, vaddr & !3, val.to_le_bytes())
                .map_err(StopReason::Exception)?;
            written_vaddr = Some(vaddr & !3);
        }

        // Branches
        Opcode::Beq => {
            ctx.result.jump = cpu.gpr[rs] == cpu.gpr[rt];
            ctx.result.jump_target = ctx
                .result
                .last_pc
                .wrapping_add(4)
                .wrapping_add_signed(imm_sext << 2);
        }
        Opcode::Bne => {
            ctx.result.jump = cpu.gpr[rs] != cpu.gpr[rt];
            ctx.result.jump_target = ctx
                .result
                .last_pc
                .wrapping_add(4)
                .wrapping_add_signed(imm_sext << 2);
        }
        Opcode::Bgez => {
            ctx.result.jump = cpu.gpr[rs].cast_signed() >= 0;
            ctx.result.jump_target = ctx
                .result
                .last_pc
                .wrapping_add(4)
                .wrapping_add_signed(imm_sext << 2);
        }
        Opcode::Blez => {
            ctx.result.jump = cpu.gpr[rs].cast_signed() <= 0;
            ctx.result.jump_target = ctx
                .result
                .last_pc
                .wrapping_add(4)
                .wrapping_add_signed(imm_sext << 2);
        }
        Opcode::Bgtz => {
            ctx.result.jump = cpu.gpr[rs].cast_signed() > 0;
            ctx.result.jump_target = ctx
                .result
                .last_pc
                .wrapping_add(4)
                .wrapping_add_signed(imm_sext << 2);
        }
        Opcode::Bltz => {
            ctx.result.jump = cpu.gpr[rs].cast_signed() < 0;
            ctx.result.jump_target = ctx
                .result
                .last_pc
                .wrapping_add(4)
                .wrapping_add_signed(imm_sext << 2);
        }
        Opcode::Bgezal => {
            cpu.write_gpr(Cpu::DEFAULT_LINK_REG, ctx.result.last_pc.wrapping_add(8));

            ctx.result.jump = cpu.gpr[rs].cast_signed() >= 0;
            ctx.result.jump_target = ctx
                .result
                .last_pc
                .wrapping_add(4)
                .wrapping_add_signed(imm_sext << 2);
        }
        Opcode::Bltzal => {
            cpu.write_gpr(Cpu::DEFAULT_LINK_REG, ctx.result.last_pc.wrapping_add(8));

            ctx.result.jump = cpu.gpr[rs].cast_signed() < 0;
            ctx.result.jump_target = ctx
                .result
                .last_pc
                .wrapping_add(4)
                .wrapping_add_signed(imm_sext << 2);
        }

        // Jumps
        Opcode::J => {
            ctx.result.jump = true;
            ctx.result.jump_target =
                (ctx.result.last_pc.wrapping_add(4) & 0xF000_0000) | (target << 2);
        }
        Opcode::Jal => {
            cpu.write_gpr(Cpu::DEFAULT_LINK_REG, ctx.result.last_pc.wrapping_add(8));

            ctx.result.jump = true;
            ctx.result.jump_target =
                (ctx.result.last_pc.wrapping_add(4) & 0xF000_0000) | (target << 2);
        }
        Opcode::Jr => {
            ctx.result.jump = true;
            ctx.result.jump_target = cpu.gpr[rs];
        }
        Opcode::Jalr => {
            cpu.write_gpr(rd, ctx.result.last_pc.wrapping_add(8));

            ctx.result.jump = true;
            ctx.result.jump_target = cpu.gpr[rs];
        }

        // MulDiv
        Opcode::Mult => {
            let a = i64::from(cpu.gpr[rs].cast_signed());
            let b = i64::from(cpu.gpr[rt].cast_signed());
            let res = (a * b).cast_unsigned();

            cpu.hi = (res >> 32) as u32;
            cpu.lo = res as u32;

            ctx.hi_lo_latency = MULT_HI_LO_LOAD_LATENCY;
        }
        Opcode::Multu => {
            let a = u64::from(cpu.gpr[rs]);
            let b = u64::from(cpu.gpr[rt]);
            let res = a * b;

            cpu.hi = (res >> 32) as u32;
            cpu.lo = res as u32;

            ctx.hi_lo_latency = MULT_HI_LO_LOAD_LATENCY;
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

            ctx.hi_lo_latency = DIV_HI_LO_LOAD_LATENCY;
        }
        Opcode::Divu => {
            let a = cpu.gpr[rs];
            let b = cpu.gpr[rt];
            let (hi, lo) = if b == 0 { (a, b) } else { (a % b, a / b) };

            cpu.hi = hi;
            cpu.lo = lo;

            ctx.hi_lo_latency = DIV_HI_LO_LOAD_LATENCY;
        }

        // From/to copies
        Opcode::Mfhi => {
            cpu.write_gpr(rd, cpu.hi);

            ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(ctx.hi_lo_latency);
            ctx.hi_lo_latency = 0;
        }
        Opcode::Mflo => {
            cpu.write_gpr(rd, cpu.lo);

            ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(ctx.hi_lo_latency);
            ctx.hi_lo_latency = 0;
        }
        Opcode::Mtlo => {
            cpu.lo = cpu.gpr[rs];
        }
        Opcode::Mthi => {
            cpu.hi = cpu.gpr[rs];
        }
        Opcode::Mfc0 => {
            pending_load = PendingLoad {
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
        Opcode::Break => return Err(StopReason::Exception(Exception::Break)),
        Opcode::Syscall => return Err(StopReason::Exception(Exception::Syscall)),
    }

    cpu.write_delayed(pending_load);

    if let Some(vaddr) = written_vaddr
        && let TranslationResult::PhysAddr(paddr) = cpu.mmu.translate_addr(vaddr)
        && let Some(invalidated_page) = ctx.blk_cache.invalidate_page(paddr)
        && ctx.block.pages.contains(&invalidated_page)
    {
        return Err(StopReason::BlockInvalidated);
    }

    Ok(())
}
