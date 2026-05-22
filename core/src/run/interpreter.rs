use core::mem;

use crate::{
    cpu::{Cpu, Exception, Opcode, PendingJump, PendingLoad, TranslationResult},
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
    cpu: &'a mut Cpu,
    bus: &'a mut Bus,
    cache: &'a mut PagedCache,
    block: &'a Block,

    result: ExecutionResult,
    hi_lo_latency: u64,
}

enum BreakReason {
    Exception(Exception),
    ControlFlow(u32),
    SelfModified,
}

pub fn run(cache: &mut PagedCache, block: &Block, cpu: &mut Cpu, bus: &mut Bus) -> ExecutionResult {
    let mut ctx = Context {
        result: ExecutionResult {
            last_pc: cpu.pc,
            next_pc: cpu.pc,
            last_in_delay_slot: cpu.pending_jump.valid,
            cycles_elapsed: 0,
            exception: None,
        },
        hi_lo_latency: 0,

        cpu,
        bus,
        cache,
        block,
    };

    for ins in &block.ops {
        match *ins {
            Operation::Instruction { pc, ins, op } => {
                ctx.result.last_pc = pc;
                ctx.result.last_in_delay_slot = ctx.cpu.pending_jump.valid;

                let res = execute(&mut ctx, ins, op);
                ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(1);
                ctx.hi_lo_latency = ctx.hi_lo_latency.saturating_sub(1);

                match res {
                    Ok(()) => {
                        ctx.result.next_pc = pc.wrapping_add(4);
                    }
                    Err(BreakReason::ControlFlow(next_pc)) => {
                        ctx.result.next_pc = next_pc;
                        break;
                    }
                    Err(BreakReason::Exception(exc)) => {
                        ctx.result.exception.replace(exc);
                        break;
                    }
                    Err(BreakReason::SelfModified) => {
                        ctx.result.next_pc = pc.wrapping_add(4);
                        break;
                    }
                }
            }
            Operation::Error { pc, cause } => {
                ctx.result.last_pc = pc;
                ctx.result.last_in_delay_slot = ctx.cpu.pending_jump.valid;
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

fn execute(ctx: &mut Context, ins: u32, op: Opcode) -> Result<(), BreakReason> {
    let rs = ((ins >> 21) & 0x1F) as usize;
    let rt = ((ins >> 16) & 0x1F) as usize;
    let rd = ((ins >> 11) & 0x1F) as usize;
    let shamt = (ins >> 6) & 0x1F;
    let imm = ins & 0xFFFF;
    let imm_sext = i32::from((imm as u16).cast_signed());
    let target = ins & 0x03FF_FFFF;

    let branch_delay_pc = {
        let old_jump = ctx.cpu.pending_jump;
        if old_jump.valid {
            if old_jump.cond {
                old_jump.then
            } else {
                old_jump.otherwise
            }
        } else {
            ctx.result.last_pc.wrapping_add(4)
        }
    };

    let mut pending_load = PendingLoad::default();
    let mut pending_jump = PendingJump::default();
    let mut written_vaddr = None;
    match op {
        // ALU ops
        Opcode::Add => {
            ctx.cpu.write_gpr(
                rd,
                ctx.cpu.gpr[rs]
                    .cast_signed()
                    .checked_add(ctx.cpu.gpr[rt].cast_signed())
                    .map(i32::cast_unsigned)
                    .ok_or(BreakReason::Exception(Exception::Overflow))?,
            );
        }
        Opcode::Addu => {
            ctx.cpu
                .write_gpr(rd, ctx.cpu.gpr[rs].wrapping_add(ctx.cpu.gpr[rt]));
        }
        Opcode::Addi => {
            ctx.cpu.write_gpr(
                rt,
                ctx.cpu.gpr[rs]
                    .cast_signed()
                    .checked_add(imm_sext)
                    .map(i32::cast_unsigned)
                    .ok_or(BreakReason::Exception(Exception::Overflow))?,
            );
        }
        Opcode::Addiu => {
            ctx.cpu
                .write_gpr(rt, ctx.cpu.gpr[rs].wrapping_add_signed(imm_sext));
        }
        Opcode::Sub => {
            ctx.cpu.write_gpr(
                rd,
                ctx.cpu.gpr[rs]
                    .cast_signed()
                    .checked_sub(ctx.cpu.gpr[rt].cast_signed())
                    .map(i32::cast_unsigned)
                    .ok_or(BreakReason::Exception(Exception::Overflow))?,
            );
        }
        Opcode::Subu => {
            ctx.cpu
                .write_gpr(rd, ctx.cpu.gpr[rs].wrapping_sub(ctx.cpu.gpr[rt]));
        }
        Opcode::And => {
            ctx.cpu.write_gpr(rd, ctx.cpu.gpr[rs] & ctx.cpu.gpr[rt]);
        }
        Opcode::Or => {
            ctx.cpu.write_gpr(rd, ctx.cpu.gpr[rs] | ctx.cpu.gpr[rt]);
        }
        Opcode::Xor => {
            ctx.cpu.write_gpr(rd, ctx.cpu.gpr[rs] ^ ctx.cpu.gpr[rt]);
        }
        Opcode::Nor => {
            ctx.cpu.write_gpr(rd, !(ctx.cpu.gpr[rs] | ctx.cpu.gpr[rt]));
        }
        Opcode::Slt => {
            ctx.cpu.write_gpr(
                rd,
                u32::from(ctx.cpu.gpr[rs].cast_signed() < ctx.cpu.gpr[rt].cast_signed()),
            );
        }
        Opcode::Sltu => {
            ctx.cpu
                .write_gpr(rd, u32::from(ctx.cpu.gpr[rs] < ctx.cpu.gpr[rt]));
        }
        Opcode::Sll => {
            ctx.cpu.write_gpr(rd, ctx.cpu.gpr[rt].wrapping_shl(shamt));
        }
        Opcode::Srl => {
            ctx.cpu.write_gpr(rd, ctx.cpu.gpr[rt].wrapping_shr(shamt));
        }
        Opcode::Sra => {
            ctx.cpu.write_gpr(
                rd,
                ctx.cpu.gpr[rt]
                    .cast_signed()
                    .wrapping_shr(shamt)
                    .cast_unsigned(),
            );
        }
        Opcode::Sllv => {
            ctx.cpu
                .write_gpr(rd, ctx.cpu.gpr[rt].wrapping_shl(ctx.cpu.gpr[rs] & 0x1F));
        }
        Opcode::Srlv => {
            ctx.cpu
                .write_gpr(rd, ctx.cpu.gpr[rt].wrapping_shr(ctx.cpu.gpr[rs] & 0x1F));
        }
        Opcode::Srav => {
            ctx.cpu.write_gpr(
                rd,
                ctx.cpu.gpr[rt]
                    .cast_signed()
                    .wrapping_shr(ctx.cpu.gpr[rs] & 0x1F)
                    .cast_unsigned(),
            );
        }
        Opcode::Slti => {
            ctx.cpu
                .write_gpr(rt, u32::from(ctx.cpu.gpr[rs].cast_signed() < imm_sext));
        }
        Opcode::Sltiu => {
            ctx.cpu
                .write_gpr(rt, u32::from(ctx.cpu.gpr[rs] < imm_sext.cast_unsigned()));
        }
        Opcode::Andi => {
            ctx.cpu.write_gpr(rt, ctx.cpu.gpr[rs] & imm);
        }
        Opcode::Ori => {
            ctx.cpu.write_gpr(rt, ctx.cpu.gpr[rs] | imm);
        }
        Opcode::Xori => {
            ctx.cpu.write_gpr(rt, ctx.cpu.gpr[rs] ^ imm);
        }
        Opcode::Lui => {
            ctx.cpu.write_gpr(rt, imm << 16);
        }

        // Loads
        Opcode::Lw => {
            pending_load = PendingLoad {
                dest: rt,
                value: ctx
                    .cpu
                    .read_bus(ctx.bus, ctx.cpu.gpr[rs].wrapping_add_signed(imm_sext))
                    .map(u32::from_le_bytes)
                    .map_err(BreakReason::Exception)?,
            };
        }
        Opcode::Lh => {
            pending_load = PendingLoad {
                dest: rt,
                value: ctx
                    .cpu
                    .read_bus(ctx.bus, ctx.cpu.gpr[rs].wrapping_add_signed(imm_sext))
                    .map(i16::from_le_bytes)
                    .map(i32::from)
                    .map(i32::cast_unsigned)
                    .map_err(BreakReason::Exception)?,
            };
        }
        Opcode::Lhu => {
            pending_load = PendingLoad {
                dest: rt,
                value: ctx
                    .cpu
                    .read_bus(ctx.bus, ctx.cpu.gpr[rs].wrapping_add_signed(imm_sext))
                    .map(u16::from_le_bytes)
                    .map(u32::from)
                    .map_err(BreakReason::Exception)?,
            };
        }
        Opcode::Lb => {
            pending_load = PendingLoad {
                dest: rt,
                value: ctx
                    .cpu
                    .read_bus(ctx.bus, ctx.cpu.gpr[rs].wrapping_add_signed(imm_sext))
                    .map(i8::from_le_bytes)
                    .map(i32::from)
                    .map(i32::cast_unsigned)
                    .map_err(BreakReason::Exception)?,
            };
        }
        Opcode::Lbu => {
            pending_load = PendingLoad {
                dest: rt,
                value: ctx
                    .cpu
                    .read_bus(ctx.bus, ctx.cpu.gpr[rs].wrapping_add_signed(imm_sext))
                    .map(u8::from_le_bytes)
                    .map(u32::from)
                    .map_err(BreakReason::Exception)?,
            };
        }
        Opcode::Lwl => {
            let addr = ctx.cpu.gpr[rs].wrapping_add_signed(imm_sext);
            let word = ctx
                .cpu
                .read_bus(ctx.bus, addr & !3)
                .map(u32::from_le_bytes)
                .map_err(BreakReason::Exception)?;
            let old = if rt == ctx.cpu.pending_load.dest {
                ctx.cpu.pending_load.value
            } else {
                ctx.cpu.gpr[rt]
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
            let addr = ctx.cpu.gpr[rs].wrapping_add_signed(imm_sext);
            let word = ctx
                .cpu
                .read_bus(ctx.bus, addr & !3)
                .map(u32::from_le_bytes)
                .map_err(BreakReason::Exception)?;
            let old = if rt == ctx.cpu.pending_load.dest {
                ctx.cpu.pending_load.value
            } else {
                ctx.cpu.gpr[rt]
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
            if ctx.cpu.cop0.status().isc() => {}

        // Stores
        Opcode::Sw => {
            let vaddr = ctx.cpu.gpr[rs].wrapping_add_signed(imm_sext);
            ctx.cpu
                .write_bus(ctx.bus, vaddr, ctx.cpu.gpr[rt].to_le_bytes())
                .map_err(BreakReason::Exception)?;
            written_vaddr = Some(vaddr);
        }
        Opcode::Sh => {
            let vaddr = ctx.cpu.gpr[rs].wrapping_add_signed(imm_sext);
            ctx.cpu
                .write_bus(ctx.bus, vaddr, (ctx.cpu.gpr[rt] as u16).to_le_bytes())
                .map_err(BreakReason::Exception)?;
            written_vaddr = Some(vaddr);
        }
        Opcode::Sb => {
            let vaddr = ctx.cpu.gpr[rs].wrapping_add_signed(imm_sext);
            ctx.cpu
                .write_bus(ctx.bus, vaddr, (ctx.cpu.gpr[rt] as u8).to_le_bytes())
                .map_err(BreakReason::Exception)?;
            written_vaddr = Some(vaddr);
        }
        Opcode::Swl => {
            let vaddr = ctx.cpu.gpr[rs].wrapping_add_signed(imm_sext);
            let word = ctx
                .cpu
                .read_bus(ctx.bus, vaddr & !3)
                .map(u32::from_le_bytes)
                .map_err(BreakReason::Exception)?;

            let val = match vaddr & 3 {
                0 => (word & 0xFFFF_FF00) | (ctx.cpu.gpr[rt] >> 24),
                1 => (word & 0xFFFF_0000) | (ctx.cpu.gpr[rt] >> 16),
                2 => (word & 0xFF00_0000) | (ctx.cpu.gpr[rt] >> 8),
                3 => ctx.cpu.gpr[rt],
                _ => unreachable!(),
            };

            ctx.cpu
                .write_bus(ctx.bus, vaddr & !3, val.to_le_bytes())
                .map_err(BreakReason::Exception)?;
            written_vaddr = Some(vaddr & !3);
        }
        Opcode::Swr => {
            let vaddr = ctx.cpu.gpr[rs].wrapping_add_signed(imm_sext);
            let word = ctx
                .cpu
                .read_bus(ctx.bus, vaddr & !3)
                .map(u32::from_le_bytes)
                .map_err(BreakReason::Exception)?;

            let val = match vaddr & 3 {
                0 => ctx.cpu.gpr[rt],
                1 => (word & 0x0000_00FF) | (ctx.cpu.gpr[rt] << 8),
                2 => (word & 0x0000_FFFF) | (ctx.cpu.gpr[rt] << 16),
                3 => (word & 0x00FF_FFFF) | (ctx.cpu.gpr[rt] << 24),
                _ => unreachable!(),
            };

            ctx.cpu
                .write_bus(ctx.bus, vaddr & !3, val.to_le_bytes())
                .map_err(BreakReason::Exception)?;
            written_vaddr = Some(vaddr & !3);
        }

        // Branches
        Opcode::Beq => {
            pending_jump = PendingJump {
                valid: true,
                cond: ctx.cpu.gpr[rs] == ctx.cpu.gpr[rt],
                then: branch_delay_pc.wrapping_add_signed(imm_sext << 2),
                otherwise: branch_delay_pc.wrapping_add(4),
            };
        }
        Opcode::Bne => {
            pending_jump = PendingJump {
                valid: true,
                cond: ctx.cpu.gpr[rs] != ctx.cpu.gpr[rt],
                then: branch_delay_pc.wrapping_add_signed(imm_sext << 2),
                otherwise: branch_delay_pc.wrapping_add(4),
            };
        }
        Opcode::Bgez => {
            pending_jump = PendingJump {
                valid: true,
                cond: ctx.cpu.gpr[rs].cast_signed() >= 0,
                then: branch_delay_pc.wrapping_add_signed(imm_sext << 2),
                otherwise: branch_delay_pc.wrapping_add(4),
            };
        }
        Opcode::Blez => {
            pending_jump = PendingJump {
                valid: true,
                cond: ctx.cpu.gpr[rs].cast_signed() <= 0,
                then: branch_delay_pc.wrapping_add_signed(imm_sext << 2),
                otherwise: branch_delay_pc.wrapping_add(4),
            };
        }
        Opcode::Bgtz => {
            pending_jump = PendingJump {
                valid: true,
                cond: ctx.cpu.gpr[rs].cast_signed() > 0,
                then: branch_delay_pc.wrapping_add_signed(imm_sext << 2),
                otherwise: branch_delay_pc.wrapping_add(4),
            };
        }
        Opcode::Bltz => {
            pending_jump = PendingJump {
                valid: true,
                cond: ctx.cpu.gpr[rs].cast_signed() < 0,
                then: branch_delay_pc.wrapping_add_signed(imm_sext << 2),
                otherwise: branch_delay_pc.wrapping_add(4),
            };
        }
        Opcode::Bgezal => {
            pending_jump = PendingJump {
                valid: true,
                cond: ctx.cpu.gpr[rs].cast_signed() >= 0,
                then: branch_delay_pc.wrapping_add_signed(imm_sext << 2),
                otherwise: branch_delay_pc.wrapping_add(4),
            };

            ctx.cpu
                .write_gpr(Cpu::DEFAULT_LINK_REG, ctx.result.last_pc.wrapping_add(8));
        }
        Opcode::Bltzal => {
            pending_jump = PendingJump {
                valid: true,
                cond: ctx.cpu.gpr[rs].cast_signed() < 0,
                then: branch_delay_pc.wrapping_add_signed(imm_sext << 2),
                otherwise: branch_delay_pc.wrapping_add(4),
            };

            ctx.cpu
                .write_gpr(Cpu::DEFAULT_LINK_REG, ctx.result.last_pc.wrapping_add(8));
        }

        // Jumps
        Opcode::J => {
            pending_jump = PendingJump {
                valid: true,
                cond: true,
                then: (branch_delay_pc & 0xF000_0000) | (target << 2),
                otherwise: (branch_delay_pc & 0xF000_0000) | (target << 2),
            };
        }
        Opcode::Jal => {
            pending_jump = PendingJump {
                valid: true,
                cond: true,
                then: (branch_delay_pc & 0xF000_0000) | (target << 2),
                otherwise: (branch_delay_pc & 0xF000_0000) | (target << 2),
            };

            ctx.cpu
                .write_gpr(Cpu::DEFAULT_LINK_REG, ctx.result.last_pc.wrapping_add(8));
        }
        Opcode::Jr => {
            pending_jump = PendingJump {
                valid: true,
                cond: true,
                then: ctx.cpu.gpr[rs],
                otherwise: ctx.cpu.gpr[rs],
            };
        }
        Opcode::Jalr => {
            pending_jump = PendingJump {
                valid: true,
                cond: true,
                then: ctx.cpu.gpr[rs],
                otherwise: ctx.cpu.gpr[rs],
            };

            ctx.cpu.write_gpr(rd, ctx.result.last_pc.wrapping_add(8));
        }

        // MulDiv
        Opcode::Mult => {
            let a = i64::from(ctx.cpu.gpr[rs].cast_signed());
            let b = i64::from(ctx.cpu.gpr[rt].cast_signed());
            let res = (a * b).cast_unsigned();

            ctx.cpu.hi = (res >> 32) as u32;
            ctx.cpu.lo = res as u32;

            ctx.hi_lo_latency = MULT_HI_LO_LOAD_LATENCY;
        }
        Opcode::Multu => {
            let a = u64::from(ctx.cpu.gpr[rs]);
            let b = u64::from(ctx.cpu.gpr[rt]);
            let res = a * b;

            ctx.cpu.hi = (res >> 32) as u32;
            ctx.cpu.lo = res as u32;

            ctx.hi_lo_latency = MULT_HI_LO_LOAD_LATENCY;
        }
        Opcode::Div => {
            let a = ctx.cpu.gpr[rs].cast_signed();
            let b = ctx.cpu.gpr[rt].cast_signed();

            let (hi, lo) = if b == 0 {
                (a.cast_unsigned(), if a < 0 { 1 } else { 0xFFFF_FFFF })
            } else if a.cast_unsigned() == 0x8000_0000 && b.cast_unsigned() == 0xFFFF_FFFF {
                (0, 0x8000_0000)
            } else {
                ((a % b).cast_unsigned(), (a / b).cast_unsigned())
            };

            ctx.cpu.hi = hi;
            ctx.cpu.lo = lo;

            ctx.hi_lo_latency = DIV_HI_LO_LOAD_LATENCY;
        }
        Opcode::Divu => {
            let a = ctx.cpu.gpr[rs];
            let b = ctx.cpu.gpr[rt];
            let (hi, lo) = if b == 0 {
                (a, 0xFFFF_FFFF)
            } else {
                (a % b, a / b)
            };

            ctx.cpu.hi = hi;
            ctx.cpu.lo = lo;

            ctx.hi_lo_latency = DIV_HI_LO_LOAD_LATENCY;
        }

        // From/to copies
        Opcode::Mfhi => {
            ctx.cpu.write_gpr(rd, ctx.cpu.hi);

            ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(ctx.hi_lo_latency);
            ctx.hi_lo_latency = 0;
        }
        Opcode::Mflo => {
            ctx.cpu.write_gpr(rd, ctx.cpu.lo);

            ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(ctx.hi_lo_latency);
            ctx.hi_lo_latency = 0;
        }
        Opcode::Mtlo => {
            ctx.cpu.lo = ctx.cpu.gpr[rs];
        }
        Opcode::Mthi => {
            ctx.cpu.hi = ctx.cpu.gpr[rs];
        }
        Opcode::Mfc0 => {
            pending_load = PendingLoad {
                dest: rt,
                value: ctx.cpu.cop0.regs[rd],
            };
        }
        Opcode::Mtc0 => {
            ctx.cpu.cop0.regs[rd] = ctx.cpu.gpr[rt];
        }
        Opcode::Cfc0 => unimplemented!(),
        Opcode::Ctc0 => unimplemented!(),

        // Return state before exception
        Opcode::Rfe => {
            ctx.cpu.cop0.exception_leave();
        }

        // Exceptions
        Opcode::Break => return Err(BreakReason::Exception(Exception::Break)),
        Opcode::Syscall => return Err(BreakReason::Exception(Exception::Syscall)),
    }

    ctx.cpu.write_delayed(pending_load);

    let invalidated_page = if let Some(vaddr) = written_vaddr
        && let TranslationResult::PhysAddr(paddr) = ctx.cpu.mmu.translate_addr(vaddr)
        && let Some(invalidated_page) = ctx.cache.invalidate_page(paddr)
    {
        Some(invalidated_page)
    } else {
        None
    };

    let PendingJump {
        valid: jump,
        cond,
        then,
        otherwise,
    } = mem::replace(&mut ctx.cpu.pending_jump, pending_jump);
    if jump {
        return Err(BreakReason::ControlFlow(if cond {
            then
        } else {
            otherwise
        }));
    }

    if let Some(invalidated_page) = invalidated_page
        && ctx.block.pages.contains(&invalidated_page)
    {
        return Err(BreakReason::SelfModified);
    }

    Ok(())
}
