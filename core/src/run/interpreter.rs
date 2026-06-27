use core::mem;

use crate::{
    cpu::{Cop0, Cpu, Exception, Instruction, PendingJump, PendingLoad, TranslationResult},
    interconnect::{Bus, Region, region_of},
};

use super::{
    ExecutionResult,
    block::{Block, PagedCache},
};

const MULT_LATENCY: u64 = 10;
const DIV_LATENCY: u64 = 35;
const LOAD_LATENCY: u64 = 3;
const STORE_LATENCY: u64 = 2;

struct Context<'a> {
    cpu: &'a mut Cpu,
    bus: &'a mut Bus,
    cache: &'a mut PagedCache,
    block: &'a Block,

    result: ExecutionResult,
}

enum BreakReason {
    Exception(Exception),
    ControlFlow(u32),
    EarlyExit,
}

#[inline(never)]
pub fn run(cache: &mut PagedCache, block: &Block, cpu: &mut Cpu, bus: &mut Bus) -> ExecutionResult {
    let mut ctx = Context {
        result: ExecutionResult {
            cycles_elapsed: 0,
            exception: None,
        },

        cpu,
        bus,
        cache,
        block,
    };

    for ins in &block.ops {
        match *ins {
            Ok(ins) => {
                let res = execute(&mut ctx, ins);
                ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(1);

                match res {
                    Ok(()) => {}
                    Err(BreakReason::EarlyExit) => {
                        ctx.cpu.pc = ctx.cpu.pc.wrapping_add(4);
                        break;
                    }
                    Err(BreakReason::ControlFlow(next_pc)) => {
                        ctx.cpu.pc = next_pc;
                        break;
                    }
                    Err(BreakReason::Exception(exc)) => {
                        ctx.result.exception.replace(exc);
                        break;
                    }
                }
            }
            Err(cause) => {
                ctx.result.exception.replace(cause);

                // Cycles
                ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(1);

                break;
            }
        }
        ctx.cpu.pc = ctx.cpu.pc.wrapping_add(4);
    }

    ctx.result
}

#[inline]
fn execute(ctx: &mut Context, ins: Instruction) -> Result<(), BreakReason> {
    let mut pending_load = PendingLoad::default();
    let mut pending_jump = PendingJump::default();
    let mut early_exit = false;
    match ins {
        // ALU ops
        Instruction::Add { rs, rt, rd } => {
            gpr_write(
                ctx.cpu,
                rd,
                gpr_read(ctx.cpu, rs)
                    .cast_signed()
                    .checked_add(gpr_read(ctx.cpu, rt).cast_signed())
                    .map(i32::cast_unsigned)
                    .ok_or(BreakReason::Exception(Exception::Overflow))?,
            );
        }
        Instruction::Addu { rs, rt, rd } => {
            gpr_write(
                ctx.cpu,
                rd,
                gpr_read(ctx.cpu, rs).wrapping_add(gpr_read(ctx.cpu, rt)),
            );
        }
        Instruction::Addi { rs, rt, imm_sext } => {
            gpr_write(
                ctx.cpu,
                rt,
                gpr_read(ctx.cpu, rs)
                    .cast_signed()
                    .checked_add(i32::from(imm_sext))
                    .map(i32::cast_unsigned)
                    .ok_or(BreakReason::Exception(Exception::Overflow))?,
            );
        }
        Instruction::Addiu { rs, rt, imm_sext } => {
            gpr_write(
                ctx.cpu,
                rt,
                gpr_read(ctx.cpu, rs).wrapping_add_signed(i32::from(imm_sext)),
            );
        }
        Instruction::Sub { rs, rt, rd } => {
            gpr_write(
                ctx.cpu,
                rd,
                gpr_read(ctx.cpu, rs)
                    .cast_signed()
                    .checked_sub(gpr_read(ctx.cpu, rt).cast_signed())
                    .map(i32::cast_unsigned)
                    .ok_or(BreakReason::Exception(Exception::Overflow))?,
            );
        }
        Instruction::Subu { rs, rt, rd } => {
            gpr_write(
                ctx.cpu,
                rd,
                gpr_read(ctx.cpu, rs).wrapping_sub(gpr_read(ctx.cpu, rt)),
            );
        }
        Instruction::And { rs, rt, rd } => {
            gpr_write(ctx.cpu, rd, gpr_read(ctx.cpu, rs) & gpr_read(ctx.cpu, rt));
        }
        Instruction::Or { rs, rt, rd } => {
            gpr_write(ctx.cpu, rd, gpr_read(ctx.cpu, rs) | gpr_read(ctx.cpu, rt));
        }
        Instruction::Xor { rs, rt, rd } => {
            gpr_write(ctx.cpu, rd, gpr_read(ctx.cpu, rs) ^ gpr_read(ctx.cpu, rt));
        }
        Instruction::Nor { rs, rt, rd } => {
            gpr_write(
                ctx.cpu,
                rd,
                !(gpr_read(ctx.cpu, rs) | gpr_read(ctx.cpu, rt)),
            );
        }
        Instruction::Slt { rs, rt, rd } => {
            gpr_write(
                ctx.cpu,
                rd,
                u32::from(
                    gpr_read(ctx.cpu, rs).cast_signed() < gpr_read(ctx.cpu, rt).cast_signed(),
                ),
            );
        }
        Instruction::Sltu { rs, rt, rd } => {
            gpr_write(
                ctx.cpu,
                rd,
                u32::from(gpr_read(ctx.cpu, rs) < gpr_read(ctx.cpu, rt)),
            );
        }
        Instruction::Sll { rt, rd, shamt } => {
            gpr_write(
                ctx.cpu,
                rd,
                gpr_read(ctx.cpu, rt).wrapping_shl(u32::from(shamt)),
            );
        }
        Instruction::Srl { rt, rd, shamt } => {
            gpr_write(
                ctx.cpu,
                rd,
                gpr_read(ctx.cpu, rt).wrapping_shr(u32::from(shamt)),
            );
        }
        Instruction::Sra { rt, rd, shamt } => {
            gpr_write(
                ctx.cpu,
                rd,
                gpr_read(ctx.cpu, rt)
                    .cast_signed()
                    .wrapping_shr(u32::from(shamt))
                    .cast_unsigned(),
            );
        }
        Instruction::Sllv { rs, rt, rd } => {
            gpr_write(
                ctx.cpu,
                rd,
                gpr_read(ctx.cpu, rt).wrapping_shl(gpr_read(ctx.cpu, rs) & 0x1F),
            );
        }
        Instruction::Srlv { rs, rt, rd } => {
            gpr_write(
                ctx.cpu,
                rd,
                gpr_read(ctx.cpu, rt).wrapping_shr(gpr_read(ctx.cpu, rs) & 0x1F),
            );
        }
        Instruction::Srav { rs, rt, rd } => {
            gpr_write(
                ctx.cpu,
                rd,
                gpr_read(ctx.cpu, rt)
                    .cast_signed()
                    .wrapping_shr(gpr_read(ctx.cpu, rs) & 0x1F)
                    .cast_unsigned(),
            );
        }
        Instruction::Slti { rs, rt, imm_sext } => {
            gpr_write(
                ctx.cpu,
                rt,
                u32::from(gpr_read(ctx.cpu, rs).cast_signed() < i32::from(imm_sext)),
            );
        }
        Instruction::Sltiu { rs, rt, imm_sext } => {
            gpr_write(
                ctx.cpu,
                rt,
                u32::from(gpr_read(ctx.cpu, rs) < i32::from(imm_sext).cast_unsigned()),
            );
        }
        Instruction::Andi { rs, rt, imm } => {
            gpr_write(ctx.cpu, rt, gpr_read(ctx.cpu, rs) & u32::from(imm));
        }
        Instruction::Ori { rs, rt, imm } => {
            gpr_write(ctx.cpu, rt, gpr_read(ctx.cpu, rs) | u32::from(imm));
        }
        Instruction::Xori { rs, rt, imm } => {
            gpr_write(ctx.cpu, rt, gpr_read(ctx.cpu, rs) ^ u32::from(imm));
        }
        Instruction::Lui { rt, imm } => {
            gpr_write(ctx.cpu, rt, u32::from(imm) << 16);
        }

        // Loads
        Instruction::Lw { rs, rt, imm_sext } => {
            pending_load = PendingLoad {
                dest: rt,
                value: ctx
                    .cpu
                    .read_bus(
                        ctx.bus,
                        gpr_read(ctx.cpu, rs).wrapping_add_signed(i32::from(imm_sext)),
                    )
                    .map(u32::from_le_bytes)
                    .map_err(BreakReason::Exception)?,
            };

            ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(LOAD_LATENCY);
        }
        Instruction::Lh { rs, rt, imm_sext } => {
            pending_load = PendingLoad {
                dest: rt,
                value: ctx
                    .cpu
                    .read_bus(
                        ctx.bus,
                        gpr_read(ctx.cpu, rs).wrapping_add_signed(i32::from(imm_sext)),
                    )
                    .map(i16::from_le_bytes)
                    .map(i32::from)
                    .map(i32::cast_unsigned)
                    .map_err(BreakReason::Exception)?,
            };

            ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(LOAD_LATENCY);
        }
        Instruction::Lhu { rs, rt, imm_sext } => {
            pending_load = PendingLoad {
                dest: rt,
                value: ctx
                    .cpu
                    .read_bus(
                        ctx.bus,
                        gpr_read(ctx.cpu, rs).wrapping_add_signed(i32::from(imm_sext)),
                    )
                    .map(u16::from_le_bytes)
                    .map(u32::from)
                    .map_err(BreakReason::Exception)?,
            };

            ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(LOAD_LATENCY);
        }
        Instruction::Lb { rs, rt, imm_sext } => {
            pending_load = PendingLoad {
                dest: rt,
                value: ctx
                    .cpu
                    .read_bus(
                        ctx.bus,
                        gpr_read(ctx.cpu, rs).wrapping_add_signed(i32::from(imm_sext)),
                    )
                    .map(i8::from_le_bytes)
                    .map(i32::from)
                    .map(i32::cast_unsigned)
                    .map_err(BreakReason::Exception)?,
            };

            ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(LOAD_LATENCY);
        }
        Instruction::Lbu { rs, rt, imm_sext } => {
            pending_load = PendingLoad {
                dest: rt,
                value: ctx
                    .cpu
                    .read_bus(
                        ctx.bus,
                        gpr_read(ctx.cpu, rs).wrapping_add_signed(i32::from(imm_sext)),
                    )
                    .map(u8::from_le_bytes)
                    .map(u32::from)
                    .map_err(BreakReason::Exception)?,
            };

            ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(LOAD_LATENCY);
        }
        Instruction::Lwl { rs, rt, imm_sext } => {
            let addr = gpr_read(ctx.cpu, rs).wrapping_add_signed(i32::from(imm_sext));
            let word = ctx
                .cpu
                .read_bus(ctx.bus, addr & !3)
                .map(u32::from_le_bytes)
                .map_err(BreakReason::Exception)?;
            let old = if rt == ctx.cpu.pending_load.dest {
                ctx.cpu.pending_load.value
            } else {
                gpr_read(ctx.cpu, rt)
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

            ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(LOAD_LATENCY);
        }
        Instruction::Lwr { rs, rt, imm_sext } => {
            let addr = gpr_read(ctx.cpu, rs).wrapping_add_signed(i32::from(imm_sext));
            let word = ctx
                .cpu
                .read_bus(ctx.bus, addr & !3)
                .map(u32::from_le_bytes)
                .map_err(BreakReason::Exception)?;
            let old = if rt == ctx.cpu.pending_load.dest {
                ctx.cpu.pending_load.value
            } else {
                gpr_read(ctx.cpu, rt)
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

            ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(LOAD_LATENCY);
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
            let vaddr = gpr_read(ctx.cpu, rs).wrapping_add_signed(i32::from(imm_sext));
            ctx.cpu
                .write_bus(ctx.bus, vaddr, gpr_read(ctx.cpu, rt).to_le_bytes())
                .map_err(BreakReason::Exception)?;

            early_exit = mem_access(ctx, vaddr);

            ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(STORE_LATENCY);
        }
        Instruction::Sh { rs, rt, imm_sext } => {
            let vaddr = gpr_read(ctx.cpu, rs).wrapping_add_signed(i32::from(imm_sext));
            ctx.cpu
                .write_bus(ctx.bus, vaddr, (gpr_read(ctx.cpu, rt) as u16).to_le_bytes())
                .map_err(BreakReason::Exception)?;

            early_exit = mem_access(ctx, vaddr);

            ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(STORE_LATENCY);
        }
        Instruction::Sb { rs, rt, imm_sext } => {
            let vaddr = gpr_read(ctx.cpu, rs).wrapping_add_signed(i32::from(imm_sext));
            ctx.cpu
                .write_bus(ctx.bus, vaddr, (gpr_read(ctx.cpu, rt) as u8).to_le_bytes())
                .map_err(BreakReason::Exception)?;

            early_exit = mem_access(ctx, vaddr);

            ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(STORE_LATENCY);
        }
        Instruction::Swl { rs, rt, imm_sext } => {
            let vaddr = gpr_read(ctx.cpu, rs).wrapping_add_signed(i32::from(imm_sext));
            let word = ctx
                .cpu
                .read_bus(ctx.bus, vaddr & !3)
                .map(u32::from_le_bytes)
                .map_err(BreakReason::Exception)?;

            let val = match vaddr & 3 {
                0 => (word & 0xFFFF_FF00) | (gpr_read(ctx.cpu, rt) >> 24),
                1 => (word & 0xFFFF_0000) | (gpr_read(ctx.cpu, rt) >> 16),
                2 => (word & 0xFF00_0000) | (gpr_read(ctx.cpu, rt) >> 8),
                3 => gpr_read(ctx.cpu, rt),
                _ => unreachable!(),
            };

            ctx.cpu
                .write_bus(ctx.bus, vaddr & !3, val.to_le_bytes())
                .map_err(BreakReason::Exception)?;

            early_exit = mem_access(ctx, vaddr);

            ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(STORE_LATENCY);
        }
        Instruction::Swr { rs, rt, imm_sext } => {
            let vaddr = gpr_read(ctx.cpu, rs).wrapping_add_signed(i32::from(imm_sext));
            let word = ctx
                .cpu
                .read_bus(ctx.bus, vaddr & !3)
                .map(u32::from_le_bytes)
                .map_err(BreakReason::Exception)?;

            let val = match vaddr & 3 {
                0 => gpr_read(ctx.cpu, rt),
                1 => (word & 0x0000_00FF) | (gpr_read(ctx.cpu, rt) << 8),
                2 => (word & 0x0000_FFFF) | (gpr_read(ctx.cpu, rt) << 16),
                3 => (word & 0x00FF_FFFF) | (gpr_read(ctx.cpu, rt) << 24),
                _ => unreachable!(),
            };

            ctx.cpu
                .write_bus(ctx.bus, vaddr & !3, val.to_le_bytes())
                .map_err(BreakReason::Exception)?;

            early_exit = mem_access(ctx, vaddr);

            ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(STORE_LATENCY);
        }

        // Branches
        Instruction::Beq { rs, rt, imm_sext } => {
            let branch_base = branch_base(ctx);
            pending_jump = PendingJump {
                valid: true,
                cond: gpr_read(ctx.cpu, rs) == gpr_read(ctx.cpu, rt),
                then: branch_base.wrapping_add_signed(i32::from(imm_sext) << 2),
                otherwise: branch_base.wrapping_add(4),
            };
        }
        Instruction::Bne { rs, rt, imm_sext } => {
            let branch_base = branch_base(ctx);
            pending_jump = PendingJump {
                valid: true,
                cond: gpr_read(ctx.cpu, rs) != gpr_read(ctx.cpu, rt),
                then: branch_base.wrapping_add_signed(i32::from(imm_sext) << 2),
                otherwise: branch_base.wrapping_add(4),
            };
        }
        Instruction::Bgez { rs, imm_sext } => {
            let branch_base = branch_base(ctx);
            pending_jump = PendingJump {
                valid: true,
                cond: gpr_read(ctx.cpu, rs).cast_signed() >= 0,
                then: branch_base.wrapping_add_signed(i32::from(imm_sext) << 2),
                otherwise: branch_base.wrapping_add(4),
            };
        }
        Instruction::Blez { rs, imm_sext } => {
            let branch_base = branch_base(ctx);
            pending_jump = PendingJump {
                valid: true,
                cond: gpr_read(ctx.cpu, rs).cast_signed() <= 0,
                then: branch_base.wrapping_add_signed(i32::from(imm_sext) << 2),
                otherwise: branch_base.wrapping_add(4),
            };
        }
        Instruction::Bgtz { rs, imm_sext } => {
            let branch_base = branch_base(ctx);
            pending_jump = PendingJump {
                valid: true,
                cond: gpr_read(ctx.cpu, rs).cast_signed() > 0,
                then: branch_base.wrapping_add_signed(i32::from(imm_sext) << 2),
                otherwise: branch_base.wrapping_add(4),
            };
        }
        Instruction::Bltz { rs, imm_sext } => {
            let branch_base = branch_base(ctx);
            pending_jump = PendingJump {
                valid: true,
                cond: gpr_read(ctx.cpu, rs).cast_signed() < 0,
                then: branch_base.wrapping_add_signed(i32::from(imm_sext) << 2),
                otherwise: branch_base.wrapping_add(4),
            };
        }
        Instruction::Bgezal { rs, imm_sext } => {
            let branch_base = branch_base(ctx);
            pending_jump = PendingJump {
                valid: true,
                cond: gpr_read(ctx.cpu, rs).cast_signed() >= 0,
                then: branch_base.wrapping_add_signed(i32::from(imm_sext) << 2),
                otherwise: branch_base.wrapping_add(4),
            };

            gpr_write(ctx.cpu, Cpu::DEFAULT_LINK_REG, ctx.cpu.pc.wrapping_add(8));
        }
        Instruction::Bltzal { rs, imm_sext } => {
            let branch_base = branch_base(ctx);
            pending_jump = PendingJump {
                valid: true,
                cond: gpr_read(ctx.cpu, rs).cast_signed() < 0,
                then: branch_base.wrapping_add_signed(i32::from(imm_sext) << 2),
                otherwise: branch_base.wrapping_add(4),
            };

            gpr_write(ctx.cpu, Cpu::DEFAULT_LINK_REG, ctx.cpu.pc.wrapping_add(8));
        }

        // Jumps
        Instruction::J { target } => {
            let branch_base = branch_base(ctx);
            let target = (branch_base & 0xF000_0000) | (target << 2);
            pending_jump = PendingJump {
                valid: true,
                cond: true,
                then: target,
                otherwise: target,
            };
        }
        Instruction::Jal { target } => {
            let branch_base = branch_base(ctx);
            let target = (branch_base & 0xF000_0000) | (target << 2);
            pending_jump = PendingJump {
                valid: true,
                cond: true,
                then: target,
                otherwise: target,
            };

            gpr_write(ctx.cpu, Cpu::DEFAULT_LINK_REG, ctx.cpu.pc.wrapping_add(8));
        }
        Instruction::Jr { rs } => {
            pending_jump = PendingJump {
                valid: true,
                cond: true,
                then: gpr_read(ctx.cpu, rs),
                otherwise: gpr_read(ctx.cpu, rs),
            };
        }
        Instruction::Jalr { rs, rd } => {
            pending_jump = PendingJump {
                valid: true,
                cond: true,
                then: gpr_read(ctx.cpu, rs),
                otherwise: gpr_read(ctx.cpu, rs),
            };

            gpr_write(ctx.cpu, rd, ctx.cpu.pc.wrapping_add(8));
        }

        // MulDiv
        Instruction::Mult { rs, rt } => {
            let a = i64::from(gpr_read(ctx.cpu, rs).cast_signed());
            let b = i64::from(gpr_read(ctx.cpu, rt).cast_signed());
            let res = (a * b).cast_unsigned();

            ctx.cpu.hi = (res >> 32) as u32;
            ctx.cpu.lo = res as u32;

            ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(MULT_LATENCY);
        }
        Instruction::Multu { rs, rt } => {
            let a = u64::from(gpr_read(ctx.cpu, rs));
            let b = u64::from(gpr_read(ctx.cpu, rt));
            let res = a * b;

            ctx.cpu.hi = (res >> 32) as u32;
            ctx.cpu.lo = res as u32;

            ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(MULT_LATENCY);
        }
        Instruction::Div { rs, rt } => {
            let a = gpr_read(ctx.cpu, rs).cast_signed();
            let b = gpr_read(ctx.cpu, rt).cast_signed();

            let (hi, lo) = if b == 0 {
                (a.cast_unsigned(), if a < 0 { 1 } else { 0xFFFF_FFFF })
            } else if a.cast_unsigned() == 0x8000_0000 && b.cast_unsigned() == 0xFFFF_FFFF {
                (0, 0x8000_0000)
            } else {
                ((a % b).cast_unsigned(), (a / b).cast_unsigned())
            };

            ctx.cpu.hi = hi;
            ctx.cpu.lo = lo;

            ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(DIV_LATENCY);
        }
        Instruction::Divu { rs, rt } => {
            let a = gpr_read(ctx.cpu, rs);
            let b = gpr_read(ctx.cpu, rt);
            let (hi, lo) = if b == 0 {
                (a, 0xFFFF_FFFF)
            } else {
                (a % b, a / b)
            };

            ctx.cpu.hi = hi;
            ctx.cpu.lo = lo;

            ctx.result.cycles_elapsed = ctx.result.cycles_elapsed.saturating_add(DIV_LATENCY);
        }

        // From/to copies
        Instruction::Mfhi { rd } => {
            gpr_write(ctx.cpu, rd, ctx.cpu.hi);
        }
        Instruction::Mflo { rd } => {
            gpr_write(ctx.cpu, rd, ctx.cpu.lo);
        }
        Instruction::Mtlo { rs } => {
            ctx.cpu.lo = gpr_read(ctx.cpu, rs);
        }
        Instruction::Mthi { rs } => {
            ctx.cpu.hi = gpr_read(ctx.cpu, rs);
        }
        Instruction::Mfc0 { rt, cop0_reg } => {
            pending_load = PendingLoad {
                dest: rt,
                value: cop0_read(ctx.cpu, cop0_reg),
            };
        }
        Instruction::Mtc0 { rt, cop0_reg } => {
            cop0_write(ctx.cpu, cop0_reg, gpr_read(ctx.cpu, rt));
            if cop0_reg == Cop0::CAUSE_IDX as u8 || cop0_reg == Cop0::STATUS_IDX as u8 {
                early_exit = true;
            }
        }
        // Return state before exception
        Instruction::Rfe => {
            ctx.cpu.cop0.exception_leave();
            early_exit = true;
        }

        // TODO
        Instruction::Mfc2 { .. }
        | Instruction::Mtc2 { .. }
        | Instruction::Cfc2 { .. }
        | Instruction::Ctc2 { .. }
        | Instruction::Cop2Cmd { .. } => {}

        // Exceptions
        Instruction::Break { .. } => return Err(BreakReason::Exception(Exception::Break)),
        Instruction::Syscall { .. } => return Err(BreakReason::Exception(Exception::Syscall)),

        _ => unimplemented!(),
    }

    pend_load(ctx.cpu, pending_load);

    // Reset on awkward write
    ctx.cpu.gpr[0] = 0;

    if let jump @ PendingJump { valid: true, .. } =
        mem::replace(&mut ctx.cpu.pending_jump, pending_jump)
    {
        return Err(BreakReason::ControlFlow(jump.target()));
    }

    if early_exit {
        return Err(BreakReason::EarlyExit);
    }

    Ok(())
}

#[inline(always)]
fn gpr_read(cpu: &Cpu, dest: u8) -> u32 {
    let dest = dest as usize;
    debug_assert!(dest < cpu.gpr.len());
    unsafe { *cpu.gpr.get_unchecked(dest) }
}

#[inline(always)]
fn gpr_write(cpu: &mut Cpu, dest: u8, value: u32) {
    let pending_load = mem::take(&mut cpu.pending_load);

    let pending_load_dest = pending_load.dest as usize;
    let pending_load_value = pending_load.value;
    debug_assert!(pending_load_dest < cpu.gpr.len());
    unsafe {
        *cpu.gpr.get_unchecked_mut(pending_load_dest) = pending_load_value;
    }

    let dest = dest as usize;
    debug_assert!(dest < cpu.gpr.len());
    unsafe {
        *cpu.gpr.get_unchecked_mut(dest) = value;
    }
}

#[inline(always)]
fn cop0_read(cpu: &Cpu, dest: u8) -> u32 {
    let dest = dest as usize;
    debug_assert!(dest < cpu.cop0.regs.len());
    unsafe { *cpu.cop0.regs.get_unchecked(dest) }
}

#[inline(always)]
fn cop0_write(cpu: &mut Cpu, dest: u8, value: u32) {
    let dest = dest as usize;
    debug_assert!(dest < cpu.cop0.regs.len());
    unsafe {
        *cpu.cop0.regs.get_unchecked_mut(dest) = value;
    }
}

#[inline(always)]
fn pend_load(cpu: &mut Cpu, new_pending_load: PendingLoad) {
    let old_pending_load = mem::replace(&mut cpu.pending_load, new_pending_load);

    if old_pending_load.dest != new_pending_load.dest {
        let dest = old_pending_load.dest as usize;
        let value = old_pending_load.value;
        debug_assert!(dest < cpu.gpr.len());
        unsafe {
            *cpu.gpr.get_unchecked_mut(dest) = value;
        }
    }
}

#[inline(always)]
fn branch_base(ctx: &Context) -> u32 {
    let old_jump = ctx.cpu.pending_jump;
    if old_jump.valid {
        old_jump.target()
    } else {
        ctx.cpu.pc.wrapping_add(4)
    }
}

#[inline(always)]
fn mem_access(ctx: &mut Context, vaddr: u32) -> bool {
    if let TranslationResult::PhysAddr(paddr) = ctx.cpu.mmu.translate_addr(vaddr) {
        if region_of(paddr) == Region::Int {
            return true;
        }
        if let Some(invalidated_page) = ctx.cache.invalidate_page(paddr)
            && ctx.block.pages.contains(&invalidated_page)
        {
            return true;
        }
    }

    false
}
