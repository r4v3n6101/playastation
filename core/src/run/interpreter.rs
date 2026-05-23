use core::mem;

use crate::{
    cpu::{Cpu, Exception, Instruction, PendingJump, PendingLoad, TranslationResult},
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
            Operation::Instruction { pc, ins } => {
                ctx.result.last_pc = pc;
                ctx.result.last_in_delay_slot = ctx.cpu.pending_jump.valid;

                let res = execute(&mut ctx, ins);
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

fn execute(ctx: &mut Context, ins: Instruction) -> Result<(), BreakReason> {
    let mut pending_load = PendingLoad::default();
    let mut pending_jump = PendingJump::default();
    let mut invalidated = false;
    match ins {
        // ALU ops
        Instruction::Add { rs, rt, rd } => {
            gpr_write(
                ctx.cpu,
                rd,
                ctx.cpu.gpr[rs]
                    .cast_signed()
                    .checked_add(ctx.cpu.gpr[rt].cast_signed())
                    .map(i32::cast_unsigned)
                    .ok_or(BreakReason::Exception(Exception::Overflow))?,
            );
        }
        Instruction::Addu { rs, rt, rd } => {
            gpr_write(ctx.cpu, rd, ctx.cpu.gpr[rs].wrapping_add(ctx.cpu.gpr[rt]));
        }
        Instruction::Addi { rs, rt, imm_sext } => {
            gpr_write(
                ctx.cpu,
                rt,
                ctx.cpu.gpr[rs]
                    .cast_signed()
                    .checked_add(imm_sext)
                    .map(i32::cast_unsigned)
                    .ok_or(BreakReason::Exception(Exception::Overflow))?,
            );
        }
        Instruction::Addiu { rs, rt, imm_sext } => {
            gpr_write(ctx.cpu, rt, ctx.cpu.gpr[rs].wrapping_add_signed(imm_sext));
        }
        Instruction::Sub { rs, rt, rd } => {
            gpr_write(
                ctx.cpu,
                rd,
                ctx.cpu.gpr[rs]
                    .cast_signed()
                    .checked_sub(ctx.cpu.gpr[rt].cast_signed())
                    .map(i32::cast_unsigned)
                    .ok_or(BreakReason::Exception(Exception::Overflow))?,
            );
        }
        Instruction::Subu { rs, rt, rd } => {
            gpr_write(ctx.cpu, rd, ctx.cpu.gpr[rs].wrapping_sub(ctx.cpu.gpr[rt]));
        }
        Instruction::And { rs, rt, rd } => {
            gpr_write(ctx.cpu, rd, ctx.cpu.gpr[rs] & ctx.cpu.gpr[rt]);
        }
        Instruction::Or { rs, rt, rd } => {
            gpr_write(ctx.cpu, rd, ctx.cpu.gpr[rs] | ctx.cpu.gpr[rt]);
        }
        Instruction::Xor { rs, rt, rd } => {
            gpr_write(ctx.cpu, rd, ctx.cpu.gpr[rs] ^ ctx.cpu.gpr[rt]);
        }
        Instruction::Nor { rs, rt, rd } => {
            gpr_write(ctx.cpu, rd, !(ctx.cpu.gpr[rs] | ctx.cpu.gpr[rt]));
        }
        Instruction::Slt { rs, rt, rd } => {
            gpr_write(
                ctx.cpu,
                rd,
                u32::from(ctx.cpu.gpr[rs].cast_signed() < ctx.cpu.gpr[rt].cast_signed()),
            );
        }
        Instruction::Sltu { rs, rt, rd } => {
            gpr_write(ctx.cpu, rd, u32::from(ctx.cpu.gpr[rs] < ctx.cpu.gpr[rt]));
        }
        Instruction::Sll { rt, rd, shamt } => {
            gpr_write(ctx.cpu, rd, ctx.cpu.gpr[rt].wrapping_shl(shamt));
        }
        Instruction::Srl { rt, rd, shamt } => {
            gpr_write(ctx.cpu, rd, ctx.cpu.gpr[rt].wrapping_shr(shamt));
        }
        Instruction::Sra { rt, rd, shamt } => {
            gpr_write(
                ctx.cpu,
                rd,
                ctx.cpu.gpr[rt]
                    .cast_signed()
                    .wrapping_shr(shamt)
                    .cast_unsigned(),
            );
        }
        Instruction::Sllv { rs, rt, rd } => {
            gpr_write(
                ctx.cpu,
                rd,
                ctx.cpu.gpr[rt].wrapping_shl(ctx.cpu.gpr[rs] & 0x1F),
            );
        }
        Instruction::Srlv { rs, rt, rd } => {
            gpr_write(
                ctx.cpu,
                rd,
                ctx.cpu.gpr[rt].wrapping_shr(ctx.cpu.gpr[rs] & 0x1F),
            );
        }
        Instruction::Srav { rs, rt, rd } => {
            gpr_write(
                ctx.cpu,
                rd,
                ctx.cpu.gpr[rt]
                    .cast_signed()
                    .wrapping_shr(ctx.cpu.gpr[rs] & 0x1F)
                    .cast_unsigned(),
            );
        }
        Instruction::Slti { rs, rt, imm_sext } => {
            gpr_write(
                ctx.cpu,
                rt,
                u32::from(ctx.cpu.gpr[rs].cast_signed() < imm_sext),
            );
        }
        Instruction::Sltiu { rs, rt, imm_sext } => {
            gpr_write(
                ctx.cpu,
                rt,
                u32::from(ctx.cpu.gpr[rs] < imm_sext.cast_unsigned()),
            );
        }
        Instruction::Andi { rs, rt, imm } => {
            gpr_write(ctx.cpu, rt, ctx.cpu.gpr[rs] & imm);
        }
        Instruction::Ori { rs, rt, imm } => {
            gpr_write(ctx.cpu, rt, ctx.cpu.gpr[rs] | imm);
        }
        Instruction::Xori { rs, rt, imm } => {
            gpr_write(ctx.cpu, rt, ctx.cpu.gpr[rs] ^ imm);
        }
        Instruction::Lui { rt, imm } => {
            gpr_write(ctx.cpu, rt, imm << 16);
        }

        // Loads
        Instruction::Lw { rs, rt, imm_sext } => {
            pending_load = PendingLoad {
                dest: rt,
                value: ctx
                    .cpu
                    .read_bus(ctx.bus, ctx.cpu.gpr[rs].wrapping_add_signed(imm_sext))
                    .map(u32::from_le_bytes)
                    .map_err(BreakReason::Exception)?,
            };
        }
        Instruction::Lh { rs, rt, imm_sext } => {
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
        Instruction::Lhu { rs, rt, imm_sext } => {
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
        Instruction::Lb { rs, rt, imm_sext } => {
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
        Instruction::Lbu { rs, rt, imm_sext } => {
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
        Instruction::Lwl { rs, rt, imm_sext } => {
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
        Instruction::Lwr { rs, rt, imm_sext } => {
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
        Instruction::Sw { .. }
        | Instruction::Sh { .. }
        | Instruction::Sb { .. }
        | Instruction::Swl { .. }
        | Instruction::Swr { .. }
            if ctx.cpu.cop0.status().isc() => {}

        // Stores
        Instruction::Sw { rs, rt, imm_sext } => {
            let vaddr = ctx.cpu.gpr[rs].wrapping_add_signed(imm_sext);
            ctx.cpu
                .write_bus(ctx.bus, vaddr, ctx.cpu.gpr[rt].to_le_bytes())
                .map_err(BreakReason::Exception)?;

            invalidated = try_invalidate_page(ctx, vaddr);
        }
        Instruction::Sh { rs, rt, imm_sext } => {
            let vaddr = ctx.cpu.gpr[rs].wrapping_add_signed(imm_sext);
            ctx.cpu
                .write_bus(ctx.bus, vaddr, (ctx.cpu.gpr[rt] as u16).to_le_bytes())
                .map_err(BreakReason::Exception)?;

            invalidated = try_invalidate_page(ctx, vaddr);
        }
        Instruction::Sb { rs, rt, imm_sext } => {
            let vaddr = ctx.cpu.gpr[rs].wrapping_add_signed(imm_sext);
            ctx.cpu
                .write_bus(ctx.bus, vaddr, (ctx.cpu.gpr[rt] as u8).to_le_bytes())
                .map_err(BreakReason::Exception)?;

            invalidated = try_invalidate_page(ctx, vaddr);
        }
        Instruction::Swl { rs, rt, imm_sext } => {
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

            invalidated = try_invalidate_page(ctx, vaddr);
        }
        Instruction::Swr { rs, rt, imm_sext } => {
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

            invalidated = try_invalidate_page(ctx, vaddr);
        }

        // Branches
        Instruction::Beq { rs, rt, imm_sext } => {
            pending_jump = PendingJump {
                valid: true,
                cond: ctx.cpu.gpr[rs] == ctx.cpu.gpr[rt],
                then: branch_base(ctx).wrapping_add_signed(imm_sext << 2),
                otherwise: branch_base(ctx).wrapping_add(4),
            };
        }
        Instruction::Bne { rs, rt, imm_sext } => {
            pending_jump = PendingJump {
                valid: true,
                cond: ctx.cpu.gpr[rs] != ctx.cpu.gpr[rt],
                then: branch_base(ctx).wrapping_add_signed(imm_sext << 2),
                otherwise: branch_base(ctx).wrapping_add(4),
            };
        }
        Instruction::Bgez { rs, imm_sext } => {
            pending_jump = PendingJump {
                valid: true,
                cond: ctx.cpu.gpr[rs].cast_signed() >= 0,
                then: branch_base(ctx).wrapping_add_signed(imm_sext << 2),
                otherwise: branch_base(ctx).wrapping_add(4),
            };
        }
        Instruction::Blez { rs, imm_sext } => {
            pending_jump = PendingJump {
                valid: true,
                cond: ctx.cpu.gpr[rs].cast_signed() <= 0,
                then: branch_base(ctx).wrapping_add_signed(imm_sext << 2),
                otherwise: branch_base(ctx).wrapping_add(4),
            };
        }
        Instruction::Bgtz { rs, imm_sext } => {
            pending_jump = PendingJump {
                valid: true,
                cond: ctx.cpu.gpr[rs].cast_signed() > 0,
                then: branch_base(ctx).wrapping_add_signed(imm_sext << 2),
                otherwise: branch_base(ctx).wrapping_add(4),
            };
        }
        Instruction::Bltz { rs, imm_sext } => {
            pending_jump = PendingJump {
                valid: true,
                cond: ctx.cpu.gpr[rs].cast_signed() < 0,
                then: branch_base(ctx).wrapping_add_signed(imm_sext << 2),
                otherwise: branch_base(ctx).wrapping_add(4),
            };
        }
        Instruction::Bgezal { rs, imm_sext } => {
            pending_jump = PendingJump {
                valid: true,
                cond: ctx.cpu.gpr[rs].cast_signed() >= 0,
                then: branch_base(ctx).wrapping_add_signed(imm_sext << 2),
                otherwise: branch_base(ctx).wrapping_add(4),
            };

            gpr_write(
                ctx.cpu,
                Cpu::DEFAULT_LINK_REG,
                ctx.result.last_pc.wrapping_add(8),
            );
        }
        Instruction::Bltzal { rs, imm_sext } => {
            pending_jump = PendingJump {
                valid: true,
                cond: ctx.cpu.gpr[rs].cast_signed() < 0,
                then: branch_base(ctx).wrapping_add_signed(imm_sext << 2),
                otherwise: branch_base(ctx).wrapping_add(4),
            };

            gpr_write(
                ctx.cpu,
                Cpu::DEFAULT_LINK_REG,
                ctx.result.last_pc.wrapping_add(8),
            );
        }

        // Jumps
        Instruction::J { target } => {
            pending_jump = PendingJump {
                valid: true,
                cond: true,
                then: (branch_base(ctx) & 0xF000_0000) | (target << 2),
                otherwise: (branch_base(ctx) & 0xF000_0000) | (target << 2),
            };
        }
        Instruction::Jal { target } => {
            pending_jump = PendingJump {
                valid: true,
                cond: true,
                then: (branch_base(ctx) & 0xF000_0000) | (target << 2),
                otherwise: (branch_base(ctx) & 0xF000_0000) | (target << 2),
            };

            gpr_write(
                ctx.cpu,
                Cpu::DEFAULT_LINK_REG,
                ctx.result.last_pc.wrapping_add(8),
            );
        }
        Instruction::Jr { rs } => {
            pending_jump = PendingJump {
                valid: true,
                cond: true,
                then: ctx.cpu.gpr[rs],
                otherwise: ctx.cpu.gpr[rs],
            };
        }
        Instruction::Jalr { rs, rd } => {
            pending_jump = PendingJump {
                valid: true,
                cond: true,
                then: ctx.cpu.gpr[rs],
                otherwise: ctx.cpu.gpr[rs],
            };

            gpr_write(ctx.cpu, rd, ctx.result.last_pc.wrapping_add(8));
        }

        // MulDiv
        Instruction::Mult { rs, rt } => {
            let a = i64::from(ctx.cpu.gpr[rs].cast_signed());
            let b = i64::from(ctx.cpu.gpr[rt].cast_signed());
            let res = (a * b).cast_unsigned();

            ctx.cpu.hi = (res >> 32) as u32;
            ctx.cpu.lo = res as u32;

            ctx.hi_lo_latency = MULT_HI_LO_LOAD_LATENCY;
        }
        Instruction::Multu { rs, rt } => {
            let a = u64::from(ctx.cpu.gpr[rs]);
            let b = u64::from(ctx.cpu.gpr[rt]);
            let res = a * b;

            ctx.cpu.hi = (res >> 32) as u32;
            ctx.cpu.lo = res as u32;

            ctx.hi_lo_latency = MULT_HI_LO_LOAD_LATENCY;
        }
        Instruction::Div { rs, rt } => {
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
        Instruction::Divu { rs, rt } => {
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
        Instruction::Mfhi { rd } => {
            gpr_write(ctx.cpu, rd, ctx.cpu.hi);

            ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(ctx.hi_lo_latency);
            ctx.hi_lo_latency = 0;
        }
        Instruction::Mflo { rd } => {
            gpr_write(ctx.cpu, rd, ctx.cpu.lo);

            ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(ctx.hi_lo_latency);
            ctx.hi_lo_latency = 0;
        }
        Instruction::Mtlo { rs } => {
            ctx.cpu.lo = ctx.cpu.gpr[rs];
        }
        Instruction::Mthi { rs } => {
            ctx.cpu.hi = ctx.cpu.gpr[rs];
        }
        Instruction::Mfc0 { rt, cop0_reg } => {
            pending_load = PendingLoad {
                dest: rt,
                value: ctx.cpu.cop0.regs[cop0_reg],
            };
        }
        Instruction::Mtc0 { rt, cop0_reg } => {
            ctx.cpu.cop0.regs[cop0_reg] = ctx.cpu.gpr[rt];
        }

        // Return state before exception
        Instruction::Rfe => {
            ctx.cpu.cop0.exception_leave();
        }

        // Exceptions
        Instruction::Break { .. } => return Err(BreakReason::Exception(Exception::Break)),
        Instruction::Syscall { .. } => return Err(BreakReason::Exception(Exception::Syscall)),

        _ => unimplemented!(),
    }

    // TODO : do only in not gpr_write ops
    pend_load(ctx.cpu, pending_load);

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

    if invalidated {
        return Err(BreakReason::SelfModified);
    }

    Ok(())
}

#[inline(always)]
fn gpr_write(cpu: &mut Cpu, dest: usize, value: u32) {
    let pending_load = mem::take(&mut cpu.pending_load);
    cpu.gpr[pending_load.dest] = pending_load.value;
    cpu.gpr[dest] = value;
    cpu.gpr[0] = 0;
}

#[inline(always)]
fn pend_load(cpu: &mut Cpu, new_pending_load: PendingLoad) {
    let old_pending_load = mem::replace(&mut cpu.pending_load, new_pending_load);

    if old_pending_load.dest != new_pending_load.dest {
        cpu.gpr[old_pending_load.dest] = old_pending_load.value;
    }

    cpu.gpr[0] = 0;
}

#[inline(always)]
fn branch_base(ctx: &Context) -> u32 {
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
}

#[inline(always)]
fn try_invalidate_page(ctx: &mut Context, vaddr: u32) -> bool {
    if let TranslationResult::PhysAddr(paddr) = ctx.cpu.mmu.translate_addr(vaddr)
        && let Some(invalidated_page) = ctx.cache.invalidate_page(paddr)
        && ctx.block.pages.contains(&invalidated_page)
    {
        true
    } else {
        false
    }
}
