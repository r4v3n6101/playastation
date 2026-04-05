use std::mem::offset_of;

use cranelift::prelude::{FunctionBuilder, InstBuilder, IntCC, MemFlags, Value, types};

use super::super::{
    super::{Cpu, ins::Opcode},
    ExecutionResult, FuncResult,
    decoder::DecRes,
};

#[allow(clippy::too_many_lines)]
pub fn emit_op(
    b: &mut FunctionBuilder,
    count: &mut u64,
    res_ptr: Value,
    ctx_ptr: Value,
    cpu_ptr: Value,
    bus_ptr: Value,
    decoded: &DecRes,
) {
    match decoded {
        &DecRes::Decoded {
            pc,
            ins,
            in_delay_slot,
            op,
        } => {
            let rs = ((ins >> 21) & 0x1F) as usize;
            let rt = ((ins >> 16) & 0x1F) as usize;
            let rd = ((ins >> 11) & 0x1F) as usize;
            let shamt = (ins >> 6) & 0x1F;
            let imm = (ins & 0xFFFF) as u16;
            let imm_sext = imm.cast_signed();
            let target = ins & 0x03FF_FFFF;

            *count += 1;
            match op {
                Opcode::Add => {
                    emit_alu_overflow_op(
                        b,
                        count,
                        res_ptr,
                        cpu_ptr,
                        pc,
                        in_delay_slot,
                        rd,
                        rs,
                        rt,
                        None,
                        |b, x, y| b.ins().sadd_overflow(x, y),
                    );
                }
                Opcode::Addu => {
                    emit_alu_op(b, cpu_ptr, rd, rs, rt, None, None, |b, x, y| {
                        b.ins().iadd(x, y)
                    });
                }
                Opcode::Sub => {
                    emit_alu_overflow_op(
                        b,
                        count,
                        res_ptr,
                        cpu_ptr,
                        pc,
                        in_delay_slot,
                        rd,
                        rs,
                        rt,
                        None,
                        |b, x, y| b.ins().ssub_overflow(x, y),
                    );
                }
                Opcode::Subu => {
                    emit_alu_op(b, cpu_ptr, rd, rs, rt, None, None, |b, x, y| {
                        b.ins().isub(x, y)
                    });
                }
                Opcode::And => {
                    emit_alu_op(b, cpu_ptr, rd, rs, rt, None, None, |b, x, y| {
                        b.ins().band(x, y)
                    });
                }
                Opcode::Or => {
                    emit_alu_op(b, cpu_ptr, rd, rs, rt, None, None, |b, x, y| {
                        b.ins().bor(x, y)
                    });
                }
                Opcode::Xor => {
                    emit_alu_op(b, cpu_ptr, rd, rs, rt, None, None, |b, x, y| {
                        b.ins().bxor(x, y)
                    });
                }
                Opcode::Nor => {
                    emit_alu_op(b, cpu_ptr, rd, rs, rt, None, None, |b, x, y| {
                        let or = b.ins().bor(x, y);
                        b.ins().bnot(or)
                    });
                }
                Opcode::Slt => {
                    emit_alu_op(b, cpu_ptr, rd, rs, rt, None, None, |b, x, y| {
                        let cond = b.ins().icmp(IntCC::SignedLessThan, x, y);
                        let one = b.ins().iconst(types::I32, 1);
                        let zero = b.ins().iconst(types::I32, 0);
                        b.ins().select(cond, one, zero)
                    });
                }
                Opcode::Sltu => {
                    emit_alu_op(b, cpu_ptr, rd, rs, rt, None, None, |b, x, y| {
                        let cond = b.ins().icmp(IntCC::UnsignedLessThan, x, y);
                        let one = b.ins().iconst(types::I32, 1);
                        let zero = b.ins().iconst(types::I32, 0);
                        b.ins().select(cond, one, zero)
                    });
                }
                Opcode::Sll => {
                    emit_alu_op(
                        b,
                        cpu_ptr,
                        rd,
                        rs,
                        rt,
                        Some(i64::from(shamt)),
                        None,
                        |b, x, y| b.ins().ishl(x, y),
                    );
                }
                Opcode::Srl => {
                    emit_alu_op(
                        b,
                        cpu_ptr,
                        rd,
                        rs,
                        rt,
                        Some(i64::from(shamt)),
                        None,
                        |b, x, y| b.ins().ushr(x, y),
                    );
                }
                Opcode::Sra => {
                    emit_alu_op(
                        b,
                        cpu_ptr,
                        rd,
                        rs,
                        rt,
                        Some(i64::from(shamt)),
                        None,
                        |b, x, y| b.ins().sshr(x, y),
                    );
                }
                Opcode::Sllv => {
                    emit_alu_op(b, cpu_ptr, rd, rs, rt, None, None, |b, x, y| {
                        let mask = b.ins().iconst(types::I32, 0x1F);
                        let var = b.ins().band(y, mask);
                        b.ins().ishl(x, var)
                    });
                }
                Opcode::Srlv => emit_alu_op(b, cpu_ptr, rd, rs, rt, None, None, |b, x, y| {
                    let mask = b.ins().iconst(types::I32, 0x1F);
                    let var = b.ins().band(y, mask);
                    b.ins().ushr(x, var)
                }),
                Opcode::Srav => {
                    emit_alu_op(b, cpu_ptr, rd, rs, rt, None, None, |b, x, y| {
                        let mask = b.ins().iconst(types::I32, 0x1F);
                        let var = b.ins().band(y, mask);
                        b.ins().sshr(x, var)
                    });
                }
                Opcode::Addi => {
                    emit_alu_overflow_op(
                        b,
                        count,
                        res_ptr,
                        cpu_ptr,
                        pc,
                        in_delay_slot,
                        rd,
                        rs,
                        rt,
                        Some(i64::from(imm_sext)),
                        |b, x, y| b.ins().sadd_overflow(x, y),
                    );
                }
                Opcode::Addiu => {
                    emit_alu_op(
                        b,
                        cpu_ptr,
                        rd,
                        rs,
                        rt,
                        None,
                        Some(i64::from(imm_sext)),
                        |b, x, y| b.ins().iadd(x, y),
                    );
                }
                Opcode::Slti => {
                    emit_alu_op(
                        b,
                        cpu_ptr,
                        rd,
                        rs,
                        rt,
                        None,
                        Some(i64::from(imm_sext)),
                        |b, x, y| {
                            let cond = b.ins().icmp(IntCC::SignedLessThan, x, y);
                            let one = b.ins().iconst(types::I32, 1);
                            let zero = b.ins().iconst(types::I32, 0);
                            b.ins().select(cond, one, zero)
                        },
                    );
                }
                Opcode::Sltiu => {
                    emit_alu_op(
                        b,
                        cpu_ptr,
                        rd,
                        rs,
                        rt,
                        None,
                        Some(i64::from(imm_sext)),
                        |b, x, y| {
                            let cond = b.ins().icmp(IntCC::UnsignedLessThan, x, y);
                            let one = b.ins().iconst(types::I32, 1);
                            let zero = b.ins().iconst(types::I32, 0);
                            b.ins().select(cond, one, zero)
                        },
                    );
                }
                Opcode::Andi => {
                    emit_alu_op(
                        b,
                        cpu_ptr,
                        rd,
                        rs,
                        rt,
                        None,
                        Some(i64::from(imm)),
                        |b, x, y| b.ins().band(x, y),
                    );
                }
                Opcode::Ori => {
                    emit_alu_op(
                        b,
                        cpu_ptr,
                        rd,
                        rs,
                        rt,
                        None,
                        Some(i64::from(imm)),
                        |b, x, y| b.ins().bor(x, y),
                    );
                }
                Opcode::Xori => {
                    emit_alu_op(
                        b,
                        cpu_ptr,
                        rd,
                        rs,
                        rt,
                        None,
                        Some(i64::from(imm)),
                        |b, x, y| b.ins().bxor(x, y),
                    );
                }
                Opcode::Lui => {
                    if rt != 0 {
                        let val = b.ins().iconst(types::I32, i64::from(imm << 16));
                        store_reg(b, cpu_ptr, Reg::General(rt), val);
                    }
                }
                // TODO : Loads
                // TODO : Stores
                // TODO : Branches
                // TODO : Jumps
                // TODO : MulDiv
                // TODO : Jumps w/ reg save
                // TODO : Mfc/mtc0
                _ => todo!(),
            }
        }
        _ => todo!(),
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_alu_op(
    b: &mut FunctionBuilder,
    cpu_ptr: Value,

    rd: usize,
    rs: usize,
    rt: usize,
    shamt: Option<i64>,
    imm: Option<i64>,
    op: impl Fn(&mut FunctionBuilder, Value, Value) -> Value,
) {
    let (out_reg, res) = if let Some(shamt) = shamt {
        if rd == 0 {
            // Zero reg is not for writing, skip the whole op
            return;
        }

        let rt_val = load_reg(b, cpu_ptr, Reg::General(rt));
        let shamt_val = b.ins().iconst(types::I32, shamt);
        (rd, op(b, rt_val, shamt_val))
    } else if let Some(imm) = imm {
        if rt == 0 {
            // Same as above
            return;
        }

        let rs_val = load_reg(b, cpu_ptr, Reg::General(rs));
        let imm_val = b.ins().iconst(types::I32, imm);
        (rt, op(b, rs_val, imm_val))
    } else {
        if rd == 0 {
            // Same as above
            return;
        }

        let rs_val = load_reg(b, cpu_ptr, Reg::General(rs));
        let rt_val = load_reg(b, cpu_ptr, Reg::General(rt));
        (rd, op(b, rs_val, rt_val))
    };

    store_reg(b, cpu_ptr, Reg::General(out_reg), res);
}

#[allow(clippy::too_many_arguments)]
fn emit_alu_overflow_op(
    b: &mut FunctionBuilder,
    count: &mut u64,
    res_ptr: Value,
    cpu_ptr: Value,

    pc: u32,
    in_delay_slot: bool,

    rd: usize,
    rs: usize,
    rt: usize,
    imm: Option<i64>,
    op: impl Fn(&mut FunctionBuilder, Value, Value) -> (Value, Value),
) {
    let (out_reg, (res, of)) = if let Some(imm) = imm {
        if rt == 0 {
            // Zero reg is not for writing, skip the whole op
            return;
        }

        let rs_val = load_reg(b, cpu_ptr, Reg::General(rs));
        let imm_val = b.ins().iconst(types::I32, imm);
        (rt, op(b, rs_val, imm_val))
    } else {
        if rd == 0 {
            // Same as above
            return;
        }

        let rs_val = load_reg(b, cpu_ptr, Reg::General(rs));
        let rt_val = load_reg(b, cpu_ptr, Reg::General(rt));
        (rd, op(b, rs_val, rt_val))
    };

    let ok = b.create_block();
    let ov = b.create_block();

    b.ins().brif(of, ov, &[], ok, &[]);

    b.switch_to_block(ov);
    emit_out_result(
        b,
        res_ptr,
        ExecutionResult::Overflow,
        pc,
        in_delay_slot,
        *count,
    );

    b.switch_to_block(ok);

    store_reg(b, cpu_ptr, Reg::General(out_reg), res);
}

fn emit_out_result(
    b: &mut FunctionBuilder,
    res_ptr: Value,
    result: ExecutionResult,
    pc: u32,
    in_delay_slot: bool,
    count: u64,
) {
    let result = b.ins().iconst(types::I32, result as i64);
    b.ins().store(
        MemFlags::new(),
        result,
        res_ptr,
        offset_of!(FuncResult, result) as i32,
    );

    let pc = b.ins().iconst(types::I32, i64::from(pc));
    b.ins().store(
        MemFlags::new(),
        pc,
        res_ptr,
        offset_of!(FuncResult, pc) as i32,
    );

    let in_delay_slot = b.ins().iconst(types::I32, i64::from(in_delay_slot));
    b.ins().store(
        MemFlags::new(),
        in_delay_slot,
        res_ptr,
        offset_of!(FuncResult, in_delay_slot) as i32,
    );

    let count = b.ins().iconst(types::I64, count.cast_signed());
    b.ins().store(
        MemFlags::new(),
        count,
        res_ptr,
        offset_of!(FuncResult, count) as i32,
    );

    // Bye!
    b.ins().return_(&[]);
}

fn load_reg(b: &mut FunctionBuilder, cpu_ptr: Value, reg: Reg) -> Value {
    b.ins().load(
        types::I32,
        MemFlags::new(),
        cpu_ptr,
        reg.byte_offset() as i32,
    )
}

fn store_reg(b: &mut FunctionBuilder, cpu_ptr: Value, reg: Reg, val: Value) {
    b.ins()
        .store(MemFlags::new(), val, cpu_ptr, reg.byte_offset() as i32);
}

#[derive(Copy, Clone)]
enum Reg {
    Pc,
    Hi,
    Lo,
    General(usize),
    Cop0(usize),
}

impl Reg {
    fn byte_offset(self) -> usize {
        match self {
            Self::Pc => offset_of!(Cpu, regs.pc),
            Self::Hi => offset_of!(Cpu, regs.hi),
            Self::Lo => offset_of!(Cpu, regs.lo),
            Self::General(idx) => offset_of!(Cpu, regs.general) + 4 * idx,
            Self::Cop0(idx) => offset_of!(Cpu, cop0.regs) + 4 * idx,
        }
    }
}
