use std::{mem::offset_of, ptr};

use cranelift::{
    module::Module,
    prelude::{FunctionBuilder, InstBuilder, IntCC, MemFlags, Value, types},
};

use super::{
    super::{
        super::{Cpu, ins::Opcode},
        ExecutionResult, FuncResult,
    },
    FnCtx,
};

pub fn emit_op(fn_ctx: &mut FnCtx, ins: u32, op: Opcode) {
    let rs = ((ins >> 21) & 0x1F) as usize;
    let rt = ((ins >> 16) & 0x1F) as usize;
    let rd = ((ins >> 11) & 0x1F) as usize;
    let shamt = (ins >> 6) & 0x1F;
    let imm = (ins & 0xFFFF) as u16;
    let imm_sext = imm.cast_signed();
    let target = ins & 0x03FF_FFFF;

    match op {
        Opcode::Add => emit_alu_overflow_op(fn_ctx, rd, rs, rt, None, |b, x, y| {
            b.ins().sadd_overflow(x, y)
        }),
        Opcode::Addu => emit_alu_op(fn_ctx, rd, rs, rt, None, None, |b, x, y| b.ins().iadd(x, y)),
        Opcode::Sub => emit_alu_overflow_op(fn_ctx, rd, rs, rt, None, |b, x, y| {
            b.ins().ssub_overflow(x, y)
        }),
        Opcode::Subu => emit_alu_op(fn_ctx, rd, rs, rt, None, None, |b, x, y| b.ins().isub(x, y)),
        Opcode::And => emit_alu_op(fn_ctx, rd, rs, rt, None, None, |b, x, y| b.ins().band(x, y)),
        Opcode::Or => emit_alu_op(fn_ctx, rd, rs, rt, None, None, |b, x, y| b.ins().bor(x, y)),
        Opcode::Xor => emit_alu_op(fn_ctx, rd, rs, rt, None, None, |b, x, y| b.ins().bxor(x, y)),
        Opcode::Nor => emit_alu_op(fn_ctx, rd, rs, rt, None, None, |b, x, y| {
            let or = b.ins().bor(x, y);
            b.ins().bnot(or)
        }),
        Opcode::Slt => emit_alu_op(fn_ctx, rd, rs, rt, None, None, |b, x, y| {
            let cond = b.ins().icmp(IntCC::SignedLessThan, x, y);
            let one = b.ins().iconst(types::I32, 1);
            let zero = b.ins().iconst(types::I32, 0);
            b.ins().select(cond, one, zero)
        }),
        Opcode::Sltu => emit_alu_op(fn_ctx, rd, rs, rt, None, None, |b, x, y| {
            let cond = b.ins().icmp(IntCC::UnsignedLessThan, x, y);
            let one = b.ins().iconst(types::I32, 1);
            let zero = b.ins().iconst(types::I32, 0);
            b.ins().select(cond, one, zero)
        }),
        Opcode::Sll => emit_alu_op(
            fn_ctx,
            rd,
            rs,
            rt,
            Some(i64::from(shamt)),
            None,
            |b, x, y| b.ins().ishl(x, y),
        ),
        Opcode::Srl => emit_alu_op(
            fn_ctx,
            rd,
            rs,
            rt,
            Some(i64::from(shamt)),
            None,
            |b, x, y| b.ins().ushr(x, y),
        ),
        Opcode::Sra => emit_alu_op(
            fn_ctx,
            rd,
            rs,
            rt,
            Some(i64::from(shamt)),
            None,
            |b, x, y| b.ins().sshr(x, y),
        ),
        Opcode::Sllv => emit_alu_op(fn_ctx, rd, rs, rt, None, None, |b, x, y| {
            let var = b.ins().band_imm(y, 0x1F);
            b.ins().ishl(x, var)
        }),
        Opcode::Srlv => emit_alu_op(fn_ctx, rd, rs, rt, None, None, |b, x, y| {
            let var = b.ins().band_imm(y, 0x1F);
            b.ins().ushr(x, var)
        }),
        Opcode::Srav => emit_alu_op(fn_ctx, rd, rs, rt, None, None, |b, x, y| {
            let var = b.ins().band_imm(y, 0x1F);
            b.ins().sshr(x, var)
        }),
        Opcode::Addi => {
            emit_alu_overflow_op(fn_ctx, rd, rs, rt, Some(i64::from(imm_sext)), |b, x, y| {
                b.ins().sadd_overflow(x, y)
            });
        }
        Opcode::Addiu => emit_alu_op(
            fn_ctx,
            rd,
            rs,
            rt,
            None,
            Some(i64::from(imm_sext)),
            |b, x, y| b.ins().iadd(x, y),
        ),
        Opcode::Slti => emit_alu_op(
            fn_ctx,
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
        ),
        Opcode::Sltiu => emit_alu_op(
            fn_ctx,
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
        ),
        Opcode::Andi => emit_alu_op(fn_ctx, rd, rs, rt, None, Some(i64::from(imm)), |b, x, y| {
            b.ins().band(x, y)
        }),
        Opcode::Ori => emit_alu_op(fn_ctx, rd, rs, rt, None, Some(i64::from(imm)), |b, x, y| {
            b.ins().bor(x, y)
        }),
        Opcode::Xori => emit_alu_op(fn_ctx, rd, rs, rt, None, Some(i64::from(imm)), |b, x, y| {
            b.ins().bxor(x, y)
        }),
        Opcode::Lui => {
            if rt != 0 {
                let val = fn_ctx
                    .builder
                    .ins()
                    .iconst(types::I32, i64::from(imm) << 16);
                store_reg(fn_ctx, Reg::General(rt), val);
            }
        }
        Opcode::Lw => {
            let rs = load_reg(fn_ctx, Reg::General(rs));
            let addr = fn_ctx.builder.ins().iadd_imm(rs, i64::from(imm_sext));
            emit_load::<4, true, 0>(fn_ctx, rt, addr);
        }
        Opcode::Lh => {
            let rs = load_reg(fn_ctx, Reg::General(rs));
            let addr = fn_ctx.builder.ins().iadd_imm(rs, i64::from(imm_sext));
            emit_load::<2, true, 0>(fn_ctx, rt, addr);
        }
        Opcode::Lhu => {
            let rs = load_reg(fn_ctx, Reg::General(rs));
            let addr = fn_ctx.builder.ins().iadd_imm(rs, i64::from(imm_sext));
            emit_load::<2, false, 0>(fn_ctx, rt, addr);
        }
        Opcode::Lb => {
            let rs = load_reg(fn_ctx, Reg::General(rs));
            let addr = fn_ctx.builder.ins().iadd_imm(rs, i64::from(imm_sext));
            emit_load::<1, true, 0>(fn_ctx, rt, addr);
        }
        Opcode::Lbu => {
            let rs = load_reg(fn_ctx, Reg::General(rs));
            let addr = fn_ctx.builder.ins().iadd_imm(rs, i64::from(imm_sext));
            emit_load::<1, false, 0>(fn_ctx, rt, addr);
        }
        Opcode::Lwl => {
            let rs = load_reg(fn_ctx, Reg::General(rs));
            let addr = fn_ctx.builder.ins().iadd_imm(rs, i64::from(imm_sext));
            emit_load::<4, false, 1>(fn_ctx, rt, addr);
        }
        Opcode::Lwr => {
            let rs = load_reg(fn_ctx, Reg::General(rs));
            let addr = fn_ctx.builder.ins().iadd_imm(rs, i64::from(imm_sext));
            emit_load::<4, false, 2>(fn_ctx, rt, addr);
        }
        Opcode::Sw => {
            let rs = load_reg(fn_ctx, Reg::General(rs));
            let addr = fn_ctx.builder.ins().iadd_imm(rs, i64::from(imm_sext));
            let val = load_reg(fn_ctx, Reg::General(rt));
            emit_store::<4, 0>(fn_ctx, addr, val);
        }
        Opcode::Sh => {
            let rs = load_reg(fn_ctx, Reg::General(rs));
            let addr = fn_ctx.builder.ins().iadd_imm(rs, i64::from(imm_sext));
            let val = load_reg(fn_ctx, Reg::General(rt));
            emit_store::<2, 0>(fn_ctx, addr, val);
        }
        Opcode::Sb => {
            let rs = load_reg(fn_ctx, Reg::General(rs));
            let addr = fn_ctx.builder.ins().iadd_imm(rs, i64::from(imm_sext));
            let val = load_reg(fn_ctx, Reg::General(rt));
            emit_store::<1, 0>(fn_ctx, addr, val);
        }
        Opcode::Swl => {
            let rs = load_reg(fn_ctx, Reg::General(rs));
            let addr = fn_ctx.builder.ins().iadd_imm(rs, i64::from(imm_sext));
            let val = load_reg(fn_ctx, Reg::General(rt));
            emit_store::<4, 1>(fn_ctx, addr, val);
        }
        Opcode::Swr => {
            let rs = load_reg(fn_ctx, Reg::General(rs));
            let addr = fn_ctx.builder.ins().iadd_imm(rs, i64::from(imm_sext));
            let val = load_reg(fn_ctx, Reg::General(rt));
            emit_store::<4, 2>(fn_ctx, addr, val);
        }

        // TODO : Branches
        // TODO : Jumps
        // TODO : MulDiv
        // TODO : Jumps w/ reg save
        // TODO : Mfc/mtc0
        _ => todo!(),
    }
}

pub fn emit_trailer(fn_ctx: &mut FnCtx) {
    emit_return(fn_ctx, Some(ExecutionResult::Success));
}

fn emit_alu_op(
    fn_ctx: &mut FnCtx,
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

        let rt_val = load_reg(fn_ctx, Reg::General(rt));
        let shamt_val = fn_ctx.builder.ins().iconst(types::I32, shamt);
        (rd, op(fn_ctx.builder, rt_val, shamt_val))
    } else if let Some(imm) = imm {
        if rt == 0 {
            // Same as above
            return;
        }

        let rs_val = load_reg(fn_ctx, Reg::General(rs));
        let imm_val = fn_ctx.builder.ins().iconst(types::I32, imm);
        (rt, op(fn_ctx.builder, rs_val, imm_val))
    } else {
        if rd == 0 {
            // Same as above
            return;
        }

        let rs_val = load_reg(fn_ctx, Reg::General(rs));
        let rt_val = load_reg(fn_ctx, Reg::General(rt));
        (rd, op(fn_ctx.builder, rs_val, rt_val))
    };

    store_reg(fn_ctx, Reg::General(out_reg), res);
}

fn emit_alu_overflow_op(
    fn_ctx: &mut FnCtx,
    rd: usize,
    rs: usize,
    rt: usize,
    imm: Option<i64>,
    op: impl Fn(&mut FunctionBuilder, Value, Value) -> (Value, Value),
) {
    let (out_reg, (res, of)) = if let Some(imm) = imm {
        if rt == 0 {
            // Same as above
            return;
        }

        let rs_val = load_reg(fn_ctx, Reg::General(rs));
        let imm_val = fn_ctx.builder.ins().iconst(types::I32, imm);
        (rt, op(fn_ctx.builder, rs_val, imm_val))
    } else {
        if rd == 0 {
            // Same as above
            return;
        }

        let rs_val = load_reg(fn_ctx, Reg::General(rs));
        let rt_val = load_reg(fn_ctx, Reg::General(rt));
        (rd, op(fn_ctx.builder, rs_val, rt_val))
    };

    let ok = fn_ctx.builder.create_block();
    let ov = fn_ctx.builder.create_block();

    fn_ctx.builder.ins().brif(of, ov, &[], ok, &[]);

    fn_ctx.builder.set_cold_block(ov);
    fn_ctx.builder.switch_to_block(ov);
    emit_return(fn_ctx, Some(ExecutionResult::Overflow));

    fn_ctx.builder.switch_to_block(ok);
    store_reg(fn_ctx, Reg::General(out_reg), res);
}

fn emit_load<const SIZE: usize, const SIGNED: bool, const DIRECTION: usize>(
    fn_ctx: &mut FnCtx,
    rt: usize,
    addr: Value,
) {
    if rt == 0 {
        return;
    }

    let ptr_ty = fn_ctx.module.target_config().pointer_type();
    let callee = fn_ctx
        .module
        .declare_func_in_func(fn_ctx.stubs.bus_load_name, fn_ctx.builder.func);

    let load_delay_reg = fn_ctx.builder.ins().iconst(
        ptr_ty,
        ptr::from_mut(&mut fn_ctx.load_delay.0).addr() as i64,
    );
    let load_delay_val = fn_ctx.builder.ins().iconst(
        ptr_ty,
        ptr::from_mut(&mut fn_ctx.load_delay.1).addr() as i64,
    );
    let dest = fn_ctx.builder.ins().iconst(types::I8, rt as i64);
    let size = fn_ctx.builder.ins().iconst(types::I8, SIZE as i64);
    let signed = fn_ctx.builder.ins().iconst(types::I8, SIGNED as i64);
    let dir = fn_ctx.builder.ins().iconst(types::I8, DIRECTION as i64);

    let call = fn_ctx.builder.ins().call(
        callee,
        &[
            fn_ctx.res_ptr,
            fn_ctx.bus_ptr,
            load_delay_reg,
            load_delay_val,
            dest,
            addr,
            size,
            signed,
            dir,
        ],
    );
    let status = fn_ctx.builder.inst_results(call)[0]; // i8

    let zero = fn_ctx.builder.ins().iconst(types::I8, 0);
    let failed = fn_ctx
        .builder
        .ins()
        .icmp(IntCC::SignedLessThan, status, zero);
    let ok = fn_ctx.builder.create_block();
    let fail = fn_ctx.builder.create_block();

    fn_ctx.builder.ins().brif(failed, fail, &[], ok, &[]);

    fn_ctx.builder.set_cold_block(fail);
    fn_ctx.builder.switch_to_block(fail);
    emit_return(fn_ctx, None);

    fn_ctx.builder.switch_to_block(ok);
}

fn emit_store<const SIZE: usize, const DIRECTION: usize>(
    fn_ctx: &mut FnCtx,
    addr: Value,
    value: Value,
) {
    let callee = fn_ctx
        .module
        .declare_func_in_func(fn_ctx.stubs.bus_store_name, fn_ctx.builder.func);

    let size = fn_ctx.builder.ins().iconst(types::I8, SIZE as i64);
    let dir = fn_ctx.builder.ins().iconst(types::I8, DIRECTION as i64);
    let call = fn_ctx.builder.ins().call(
        callee,
        &[
            fn_ctx.res_ptr,
            fn_ctx.cpu_ptr,
            fn_ctx.bus_ptr,
            addr,
            value,
            size,
            dir,
        ],
    );
    let status = fn_ctx.builder.inst_results(call)[0]; // i8

    let zero = fn_ctx.builder.ins().iconst(types::I8, 0);
    let failed = fn_ctx
        .builder
        .ins()
        .icmp(IntCC::SignedLessThan, status, zero);
    let ok = fn_ctx.builder.create_block();
    let fail = fn_ctx.builder.create_block();

    fn_ctx.builder.ins().brif(failed, fail, &[], ok, &[]);

    fn_ctx.builder.set_cold_block(fail);
    fn_ctx.builder.switch_to_block(fail);
    emit_return(fn_ctx, None);

    fn_ctx.builder.switch_to_block(ok);
}

fn emit_return(fn_ctx: &mut FnCtx, result: Option<ExecutionResult>) {
    if let Some(result) = result {
        let result = fn_ctx.builder.ins().iconst(types::I32, result as i64);
        fn_ctx.builder.ins().store(
            MemFlags::new(),
            result,
            fn_ctx.res_ptr,
            offset_of!(FuncResult, result) as i32,
        );
    }

    let pc = fn_ctx
        .builder
        .ins()
        .iconst(types::I32, i64::from(fn_ctx.last_pc));
    fn_ctx.builder.ins().store(
        MemFlags::new(),
        pc,
        fn_ctx.res_ptr,
        offset_of!(FuncResult, last_pc) as i32,
    );

    let in_delay_slot = fn_ctx
        .builder
        .ins()
        .iconst(types::I32, i64::from(fn_ctx.last_in_delay_slot));
    fn_ctx.builder.ins().store(
        MemFlags::new(),
        in_delay_slot,
        fn_ctx.res_ptr,
        offset_of!(FuncResult, last_in_delay_slot) as i32,
    );

    // Bye!
    fn_ctx.builder.ins().return_(&[]);
}

fn load_reg(fn_ctx: &mut FnCtx, reg: Reg) -> Value {
    fn_ctx.builder.ins().load(
        types::I32,
        MemFlags::new(),
        fn_ctx.cpu_ptr,
        reg.byte_offset() as i32,
    )
}

fn store_reg(fn_ctx: &mut FnCtx, reg: Reg, val: Value) {
    fn_ctx.builder.ins().store(
        MemFlags::new(),
        val,
        fn_ctx.cpu_ptr,
        reg.byte_offset() as i32,
    );
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
