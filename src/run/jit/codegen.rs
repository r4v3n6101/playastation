use std::mem::offset_of;

use cranelift::{
    jit::JITModule,
    module::{FuncId, Linkage, Module},
    prelude::{AbiParam, FunctionBuilder, InstBuilder, IntCC, MemFlags, Value, types},
};

use crate::cpu::{Cpu, Exception, Opcode};

use super::{ExecutionResult, FuncResult, Storage, stubs};

pub struct FnCtx<'module> {
    module: &'module JITModule,
    /// If val is true then the commit of load-delay slot will be generated
    pending_load_delay_gen: bool,
    /// Function builder
    builder: FunctionBuilder<'module>,
    /// Function name in form of an Id
    name: FuncId,
    /// Output result
    res_ptr: Value,
    /// CPU state
    cpu_ptr: Value,
    /// Bus
    bus_ptr: Value,
    /// PC of last emitted op
    last_pc: u32,
    /// Same as above, but for delay slot
    last_in_delay_slot: bool,
}

impl<'a> FnCtx<'a> {
    pub fn create_and_emit_header(storage: &'a mut Storage, enter_pc: u32) -> Self {
        let ptr_ty = storage.module.target_config().pointer_type();

        let mut sig = storage.module.make_signature();
        sig.params.push(AbiParam::new(ptr_ty)); // *mut res
        sig.params.push(AbiParam::new(ptr_ty)); // *mut cpu
        sig.params.push(AbiParam::new(ptr_ty)); // *mut bus

        let name = storage
            .module
            .declare_function(&format!("enter_{enter_pc:#}"), Linkage::Local, &sig)
            .unwrap();

        let mut builder = FunctionBuilder::new(&mut storage.ctx.func, &mut storage.fn_build_ctx);
        builder.func.signature = sig;

        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);

        let res_ptr = builder.block_params(entry)[0];
        let cpu_ptr = builder.block_params(entry)[1];
        let bus_ptr = builder.block_params(entry)[2];

        Self {
            builder,
            name,
            res_ptr,
            cpu_ptr,
            bus_ptr,
            last_pc: enter_pc,
            last_in_delay_slot: false,
            module: &storage.module,
            pending_load_delay_gen: true,
        }
    }

    pub fn emit_trailer(&mut self) {
        self.emit_return(None, None, None);
    }

    pub fn finalize(mut self) -> FuncId {
        self.builder.seal_all_blocks();
        self.builder.finalize();

        self.name
    }

    pub fn emit_exception(&mut self, pc: u32, in_delay_slot: bool, exc: Exception) {
        self.last_pc = pc;
        self.last_in_delay_slot = in_delay_slot;

        let load_delay_slot = if self.pending_load_delay_gen {
            self.pending_load_delay_gen = false;
            Some(self.read_pending_load())
        } else {
            None
        };

        let (res, bad_vaddr) = match exc {
            Exception::UnalignedLoad { bad_vaddr } => {
                (ExecutionResult::UnalignedLoad, Some(bad_vaddr))
            }
            Exception::UnalignedStore { bad_vaddr } => {
                (ExecutionResult::UnalignedStore, Some(bad_vaddr))
            }
            Exception::InstructionBus { bad_vaddr } => {
                (ExecutionResult::InstructionBus, Some(bad_vaddr))
            }
            Exception::DataBus { bad_vaddr } => (ExecutionResult::DataBus, Some(bad_vaddr)),
            Exception::Syscall => (ExecutionResult::Syscall, None),
            Exception::Break => (ExecutionResult::Break, None),
            Exception::ReservedInstruction => (ExecutionResult::ReservedInstruction, None),
            // Other never parsed or handled from another place
            _ => unreachable!(),
        };
        self.emit_return(Some(res), bad_vaddr, load_delay_slot);
    }

    pub fn emit_op(&mut self, pc: u32, in_delay_slot: bool, ins: u32, op: Opcode) {
        let ptr_ty = self.module.target_config().pointer_type();

        let rs = ((ins >> 21) & 0x1F) as usize;
        let rt = ((ins >> 16) & 0x1F) as usize;
        let rd = ((ins >> 11) & 0x1F) as usize;
        let shamt = (ins >> 6) & 0x1F;
        let imm = (ins & 0xFFFF) as u16;
        let imm_sext = imm.cast_signed();
        let target = ins & 0x03FF_FFFF;
        // Complete address
        let jump_target = (pc.wrapping_add(4) & 0xF000_0000) | (target << 2);
        let branch_target = pc
            .wrapping_add(4)
            .wrapping_add_signed(i32::from(imm_sext) << 2);

        self.last_pc = pc;
        self.last_in_delay_slot = in_delay_slot;

        let load_delay_slot = if self.pending_load_delay_gen {
            self.pending_load_delay_gen = false;
            Some(self.read_pending_load())
        } else {
            None
        };
        match op {
            Opcode::Add => self.emit_alu_overflow_op(
                rd,
                rs,
                rt,
                None,
                |b, x, y| b.ins().sadd_overflow(x, y),
                load_delay_slot,
            ),
            Opcode::Addu => self.emit_alu_op(rd, rs, rt, None, None, |b, x, y| b.ins().iadd(x, y)),
            Opcode::Sub => self.emit_alu_overflow_op(
                rd,
                rs,
                rt,
                None,
                |b, x, y| b.ins().ssub_overflow(x, y),
                load_delay_slot,
            ),
            Opcode::Subu => self.emit_alu_op(rd, rs, rt, None, None, |b, x, y| b.ins().isub(x, y)),
            Opcode::And => self.emit_alu_op(rd, rs, rt, None, None, |b, x, y| b.ins().band(x, y)),
            Opcode::Or => self.emit_alu_op(rd, rs, rt, None, None, |b, x, y| b.ins().bor(x, y)),
            Opcode::Xor => self.emit_alu_op(rd, rs, rt, None, None, |b, x, y| b.ins().bxor(x, y)),
            Opcode::Nor => self.emit_alu_op(rd, rs, rt, None, None, |b, x, y| {
                let or = b.ins().bor(x, y);
                b.ins().bnot(or)
            }),
            Opcode::Slt => self.emit_alu_op(rd, rs, rt, None, None, |b, x, y| {
                let cond = b.ins().icmp(IntCC::SignedLessThan, x, y);
                let one = b.ins().iconst(types::I32, 1);
                let zero = b.ins().iconst(types::I32, 0);
                b.ins().select(cond, one, zero)
            }),
            Opcode::Sltu => self.emit_alu_op(rd, rs, rt, None, None, |b, x, y| {
                let cond = b.ins().icmp(IntCC::UnsignedLessThan, x, y);
                let one = b.ins().iconst(types::I32, 1);
                let zero = b.ins().iconst(types::I32, 0);
                b.ins().select(cond, one, zero)
            }),
            Opcode::Sll => self.emit_alu_op(rd, rs, rt, Some(i64::from(shamt)), None, |b, x, y| {
                b.ins().ishl(x, y)
            }),
            Opcode::Srl => self.emit_alu_op(rd, rs, rt, Some(i64::from(shamt)), None, |b, x, y| {
                b.ins().ushr(x, y)
            }),
            Opcode::Sra => self.emit_alu_op(rd, rs, rt, Some(i64::from(shamt)), None, |b, x, y| {
                b.ins().sshr(x, y)
            }),
            Opcode::Sllv => self.emit_alu_op(rd, rs, rt, None, None, |b, x, y| {
                let var = b.ins().band_imm(y, 0x1F);
                b.ins().ishl(x, var)
            }),
            Opcode::Srlv => self.emit_alu_op(rd, rs, rt, None, None, |b, x, y| {
                let var = b.ins().band_imm(y, 0x1F);
                b.ins().ushr(x, var)
            }),
            Opcode::Srav => self.emit_alu_op(rd, rs, rt, None, None, |b, x, y| {
                let var = b.ins().band_imm(y, 0x1F);
                b.ins().sshr(x, var)
            }),
            Opcode::Addi => {
                self.emit_alu_overflow_op(
                    rd,
                    rs,
                    rt,
                    Some(i64::from(imm_sext)),
                    |b, x, y| b.ins().sadd_overflow(x, y),
                    load_delay_slot,
                );
            }
            Opcode::Addiu => {
                self.emit_alu_op(rd, rs, rt, None, Some(i64::from(imm_sext)), |b, x, y| {
                    b.ins().iadd(x, y)
                })
            }
            Opcode::Slti => {
                self.emit_alu_op(rd, rs, rt, None, Some(i64::from(imm_sext)), |b, x, y| {
                    let cond = b.ins().icmp(IntCC::SignedLessThan, x, y);
                    let one = b.ins().iconst(types::I32, 1);
                    let zero = b.ins().iconst(types::I32, 0);
                    b.ins().select(cond, one, zero)
                })
            }
            Opcode::Sltiu => {
                self.emit_alu_op(rd, rs, rt, None, Some(i64::from(imm_sext)), |b, x, y| {
                    let cond = b.ins().icmp(IntCC::UnsignedLessThan, x, y);
                    let one = b.ins().iconst(types::I32, 1);
                    let zero = b.ins().iconst(types::I32, 0);
                    b.ins().select(cond, one, zero)
                })
            }
            Opcode::Andi => self.emit_alu_op(rd, rs, rt, None, Some(i64::from(imm)), |b, x, y| {
                b.ins().band(x, y)
            }),
            Opcode::Ori => self.emit_alu_op(rd, rs, rt, None, Some(i64::from(imm)), |b, x, y| {
                b.ins().bor(x, y)
            }),
            Opcode::Xori => self.emit_alu_op(rd, rs, rt, None, Some(i64::from(imm)), |b, x, y| {
                b.ins().bxor(x, y)
            }),
            Opcode::Lui => {
                if rt != 0 {
                    let val = self.builder.ins().iconst(types::I32, i64::from(imm) << 16);
                    self.store_reg(Reg::General(rt), val);
                }
            }
            Opcode::Lw => {
                let rs = self.load_reg(Reg::General(rs));
                let addr = self.builder.ins().iadd_imm(rs, i64::from(imm_sext));
                self.emit_load(
                    stubs::bus_load::<4, true> as *const u8,
                    rt,
                    addr,
                    load_delay_slot,
                );
            }
            Opcode::Lh => {
                let rs = self.load_reg(Reg::General(rs));
                let addr = self.builder.ins().iadd_imm(rs, i64::from(imm_sext));
                self.emit_load(
                    stubs::bus_load::<2, true> as *const u8,
                    rt,
                    addr,
                    load_delay_slot,
                );
            }
            Opcode::Lhu => {
                let rs = self.load_reg(Reg::General(rs));
                let addr = self.builder.ins().iadd_imm(rs, i64::from(imm_sext));
                self.emit_load(
                    stubs::bus_load::<2, false> as *const u8,
                    rt,
                    addr,
                    load_delay_slot,
                );
            }
            Opcode::Lb => {
                let rs = self.load_reg(Reg::General(rs));
                let addr = self.builder.ins().iadd_imm(rs, i64::from(imm_sext));
                self.emit_load(
                    stubs::bus_load::<1, true> as *const u8,
                    rt,
                    addr,
                    load_delay_slot,
                );
            }
            Opcode::Lbu => {
                let rs = self.load_reg(Reg::General(rs));
                let addr = self.builder.ins().iadd_imm(rs, i64::from(imm_sext));
                self.emit_load(
                    stubs::bus_load::<1, false> as *const u8,
                    rt,
                    addr,
                    load_delay_slot,
                );
            }
            Opcode::Lwl => {
                let rs = self.load_reg(Reg::General(rs));
                let addr = self.builder.ins().iadd_imm(rs, i64::from(imm_sext));
                todo!()
            }
            Opcode::Lwr => {
                let rs = self.load_reg(Reg::General(rs));
                let addr = self.builder.ins().iadd_imm(rs, i64::from(imm_sext));
                todo!()
            }
            Opcode::Sw => {
                let rs = self.load_reg(Reg::General(rs));
                let addr = self.builder.ins().iadd_imm(rs, i64::from(imm_sext));
                let val = self.load_reg(Reg::General(rt));
                self.emit_store(
                    stubs::bus_store::<4> as *const u8,
                    addr,
                    val,
                    load_delay_slot,
                );
            }
            Opcode::Sh => {
                let rs = self.load_reg(Reg::General(rs));
                let addr = self.builder.ins().iadd_imm(rs, i64::from(imm_sext));
                let val = self.load_reg(Reg::General(rt));
                self.emit_store(
                    stubs::bus_store::<2> as *const u8,
                    addr,
                    val,
                    load_delay_slot,
                );
            }
            Opcode::Sb => {
                let rs = self.load_reg(Reg::General(rs));
                let addr = self.builder.ins().iadd_imm(rs, i64::from(imm_sext));
                let val = self.load_reg(Reg::General(rt));
                self.emit_store(
                    stubs::bus_store::<1> as *const u8,
                    addr,
                    val,
                    load_delay_slot,
                );
            }
            Opcode::Swl => {
                let rs = self.load_reg(Reg::General(rs));
                let addr = self.builder.ins().iadd_imm(rs, i64::from(imm_sext));
                let val = self.load_reg(Reg::General(rt));
                todo!()
            }
            Opcode::Swr => {
                let rs = self.load_reg(Reg::General(rs));
                let addr = self.builder.ins().iadd_imm(rs, i64::from(imm_sext));
                let val = self.load_reg(Reg::General(rt));
                todo!()
            }

            Opcode::Beq => {
                self.emit_branch(branch_target, false, IntCC::Equal, rs, rt);
            }
            Opcode::Bne => {
                self.emit_branch(branch_target, false, IntCC::NotEqual, rs, rt);
            }
            Opcode::Bltz => {
                self.emit_branch(branch_target, false, IntCC::SignedLessThan, rs, 0);
            }
            Opcode::Blez => {
                self.emit_branch(branch_target, false, IntCC::SignedLessThanOrEqual, rs, 0);
            }
            Opcode::Bgtz => {
                self.emit_branch(branch_target, false, IntCC::SignedGreaterThan, rs, 0);
            }
            Opcode::Bgez => {
                self.emit_branch(branch_target, false, IntCC::SignedGreaterThanOrEqual, rs, 0);
            }
            Opcode::Bltzal => {
                self.emit_branch(branch_target, true, IntCC::SignedLessThan, rs, 0);
            }
            Opcode::Bgezal => {
                self.emit_branch(branch_target, true, IntCC::SignedGreaterThanOrEqual, rs, 0);
            }

            Opcode::J => {
                let target = self
                    .builder
                    .ins()
                    .iconst(types::I32, i64::from(jump_target.cast_signed()));
                self.emit_jump(target, None);
            }
            Opcode::Jal => {
                let target = self
                    .builder
                    .ins()
                    .iconst(types::I32, i64::from(jump_target.cast_signed()));
                self.emit_jump(target, Some(Cpu::DEFAULT_LINK_REG));
            }
            Opcode::Jr => {
                let target = self.load_reg(Reg::General(rs));
                self.emit_jump(target, None);
            }
            Opcode::Jalr => {
                let target = self.load_reg(Reg::General(rs));
                self.emit_jump(target, Some(rd));
            }

            Opcode::Mult => {
                let rs = {
                    let reg = self.load_reg(Reg::General(rs));
                    self.builder.ins().sextend(types::I64, reg)
                };
                let rt = {
                    let reg = self.load_reg(Reg::General(rt));
                    self.builder.ins().sextend(types::I64, reg)
                };
                let hi = self.builder.ins().smulhi(rs, rt);
                let lo = self.builder.ins().imul(rs, rt);

                self.store_reg(Reg::Hi, hi);
                self.store_reg(Reg::Lo, lo);
            }
            Opcode::Multu => {
                let rs = self.load_reg(Reg::General(rs));
                let rt = self.load_reg(Reg::General(rt));
                let hi = self.builder.ins().smulhi(rs, rt);
                let lo = self.builder.ins().imul(rs, rt);

                self.store_reg(Reg::Hi, hi);
                self.store_reg(Reg::Lo, lo);
            }
            Opcode::Div => {
                let rs_val = self.load_reg(Reg::General(rs));
                let rt_val = self.load_reg(Reg::General(rt));

                let div_by_zero = self.builder.ins().icmp_imm(IntCC::Equal, rt_val, 0);
                let is_min =
                    self.builder
                        .ins()
                        .icmp_imm(IntCC::Equal, rs_val, 0x8000_0000u64.cast_signed());
                let is_neg1 = self.builder.ins().icmp_imm(IntCC::Equal, rt_val, -1);
                let overflow = self.builder.ins().band(is_min, is_neg1);
                let special = self.builder.ins().bor(div_by_zero, overflow);

                let normal_block = self.builder.create_block();
                let special_block = self.builder.create_block();
                let done = self.builder.create_block();

                self.builder
                    .ins()
                    .brif(special, special_block, &[], normal_block, &[]);
                {
                    self.builder.switch_to_block(special_block);
                    self.store_reg(Reg::Hi, rs_val);
                    self.store_reg(Reg::Lo, rt_val);
                    self.builder.ins().jump(done, &[]);
                }
                {
                    self.builder.switch_to_block(normal_block);
                    let lo = self.builder.ins().sdiv(rs_val, rt_val);
                    let hi = self.builder.ins().srem(rs_val, rt_val);
                    self.store_reg(Reg::Lo, lo);
                    self.store_reg(Reg::Hi, hi);
                    self.builder.ins().jump(done, &[]);
                }
                self.builder.switch_to_block(done);
            }
            Opcode::Divu => {
                let rs_val = self.load_reg(Reg::General(rs));
                let rt_val = self.load_reg(Reg::General(rt));

                let div_by_zero = self.builder.ins().icmp_imm(IntCC::Equal, rt_val, 0);

                let normal_block = self.builder.create_block();
                let special_block = self.builder.create_block();
                let done = self.builder.create_block();

                self.builder
                    .ins()
                    .brif(div_by_zero, special_block, &[], normal_block, &[]);
                {
                    self.builder.switch_to_block(special_block);
                    self.store_reg(Reg::Hi, rs_val);
                    self.store_reg(Reg::Lo, rt_val);
                    self.builder.ins().jump(done, &[]);
                }
                {
                    self.builder.switch_to_block(normal_block);
                    let lo = self.builder.ins().udiv(rs_val, rt_val);
                    let hi = self.builder.ins().urem(rs_val, rt_val);
                    self.store_reg(Reg::Lo, lo);
                    self.store_reg(Reg::Hi, hi);
                    self.builder.ins().jump(done, &[]);
                }
                self.builder.switch_to_block(done);
            }

            Opcode::Mfhi => {
                let hi = self.load_reg(Reg::Hi);
                self.store_reg(Reg::General(rd), hi);
            }
            Opcode::Mflo => {
                let lo = self.load_reg(Reg::Lo);
                self.store_reg(Reg::General(rd), lo);
            }
            Opcode::Mthi => {
                let rs = self.load_reg(Reg::General(rs));
                self.store_reg(Reg::Hi, rs);
            }
            Opcode::Mtlo => {
                let rs = self.load_reg(Reg::General(rs));
                self.store_reg(Reg::Lo, rs);
            }
            Opcode::Mfc0 => {
                let dest = self.builder.ins().iconst(ptr_ty, rt.cast_signed() as i64);
                let rd = self.load_reg(Reg::Cop0(rd));

                self.set_pending_load(dest, rd);
                self.pending_load_delay_gen = true;
            }
            Opcode::Mtc0 => {
                let rt = self.load_reg(Reg::General(rt));
                self.store_reg(Reg::Cop0(rd), rt);
            }

            Opcode::Rfe => {
                let fn_ptr = self.builder.ins().iconst(
                    ptr_ty,
                    (stubs::rfe as *const u8).addr().cast_signed() as i64,
                );

                let mut sig = self.module.make_signature();
                sig.params.push(AbiParam::new(ptr_ty));

                let sigref = self.builder.import_signature(sig);
                self.builder
                    .ins()
                    .call_indirect(sigref, fn_ptr, &[self.cpu_ptr]);
            }
            _ => todo!(),
        }
        if let Some(load_delay_slot) = load_delay_slot {
            self.commit_load_delay(load_delay_slot);
        }
    }

    fn emit_alu_op(
        &mut self,
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

            let rt_val = self.load_reg(Reg::General(rt));
            let shamt_val = self.builder.ins().iconst(types::I32, shamt);
            (rd, op(&mut self.builder, rt_val, shamt_val))
        } else if let Some(imm) = imm {
            if rt == 0 {
                // Same as above
                return;
            }

            let rs_val = self.load_reg(Reg::General(rs));
            let imm_val = self.builder.ins().iconst(types::I32, imm);
            (rt, op(&mut self.builder, rs_val, imm_val))
        } else {
            if rd == 0 {
                // Same as above
                return;
            }

            let rs_val = self.load_reg(Reg::General(rs));
            let rt_val = self.load_reg(Reg::General(rt));
            (rd, op(&mut self.builder, rs_val, rt_val))
        };

        self.store_reg(Reg::General(out_reg), res);
    }

    fn emit_alu_overflow_op(
        &mut self,
        rd: usize,
        rs: usize,
        rt: usize,
        imm: Option<i64>,
        op: impl Fn(&mut FunctionBuilder, Value, Value) -> (Value, Value),
        load_delay_slot: Option<(Value, Value)>,
    ) {
        let (out_reg, (res, of)) = if let Some(imm) = imm {
            if rt == 0 {
                // Same as above
                return;
            }

            let rs_val = self.load_reg(Reg::General(rs));
            let imm_val = self.builder.ins().iconst(types::I32, imm);
            (rt, op(&mut self.builder, rs_val, imm_val))
        } else {
            if rd == 0 {
                // Same as above
                return;
            }

            let rs_val = self.load_reg(Reg::General(rs));
            let rt_val = self.load_reg(Reg::General(rt));
            (rd, op(&mut self.builder, rs_val, rt_val))
        };

        let ok = self.builder.create_block();
        let ov = self.builder.create_block();

        self.builder.ins().brif(of, ov, &[], ok, &[]);
        {
            self.builder.set_cold_block(ov);
            self.builder.switch_to_block(ov);
            self.emit_return(Some(ExecutionResult::Overflow), None, load_delay_slot);
        }
        {
            self.builder.switch_to_block(ok);
            self.store_reg(Reg::General(out_reg), res);
        }
    }

    fn emit_load(
        &mut self,
        fn_ptr: *const u8,
        rt: usize,
        addr: Value,
        load_delay_slot: Option<(Value, Value)>,
    ) {
        if rt == 0 {
            return;
        }

        let ptr_ty = self.module.target_config().pointer_type();

        let fn_ptr = self
            .builder
            .ins()
            .iconst(ptr_ty, fn_ptr.addr().cast_signed() as i64);
        let dest = self.builder.ins().iconst(ptr_ty, rt.cast_signed() as i64);
        let call = {
            let mut sig = self.module.make_signature();
            sig.params.push(AbiParam::new(ptr_ty));
            sig.params.push(AbiParam::new(ptr_ty));
            sig.params.push(AbiParam::new(ptr_ty));
            sig.params.push(AbiParam::new(ptr_ty));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::I8));

            let sigref = self.builder.import_signature(sig);
            self.builder.ins().call_indirect(
                sigref,
                fn_ptr,
                &[self.res_ptr, self.cpu_ptr, self.bus_ptr, dest, addr],
            )
        };
        let result = self.builder.inst_results(call)[0];

        let failed = self
            .builder
            .ins()
            .icmp_imm(IntCC::SignedLessThan, result, 0);
        let ok = self.builder.create_block();
        let fail = self.builder.create_block();

        self.builder.ins().brif(failed, fail, &[], ok, &[]);
        {
            self.builder.set_cold_block(fail);
            self.builder.switch_to_block(fail);
            self.emit_return(None, None, load_delay_slot);
        }
        {
            self.builder.switch_to_block(ok);
        }
        // Store load-delayed slot into regs will be generated
        self.pending_load_delay_gen = true;
    }

    fn emit_store(
        &mut self,
        fn_ptr: *const u8,
        addr: Value,
        value: Value,
        load_delay_slot: Option<(Value, Value)>,
    ) {
        let ptr_ty = self.module.target_config().pointer_type();

        let fn_ptr = self
            .builder
            .ins()
            .iconst(ptr_ty, fn_ptr.addr().cast_signed() as i64);

        let call = {
            let mut sig = self.module.make_signature();
            sig.params.push(AbiParam::new(ptr_ty));
            sig.params.push(AbiParam::new(ptr_ty));
            sig.params.push(AbiParam::new(ptr_ty));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.returns.push(AbiParam::new(types::I8));

            let sigref = self.builder.import_signature(sig);
            self.builder.ins().call_indirect(
                sigref,
                fn_ptr,
                &[self.res_ptr, self.cpu_ptr, self.bus_ptr, addr, value],
            )
        };
        let status = self.builder.inst_results(call)[0];

        let failed = self
            .builder
            .ins()
            .icmp_imm(IntCC::SignedLessThan, status, 0);
        let ok = self.builder.create_block();
        let fail = self.builder.create_block();

        self.builder.ins().brif(failed, fail, &[], ok, &[]);
        {
            self.builder.set_cold_block(fail);
            self.builder.switch_to_block(fail);
            self.emit_return(None, None, load_delay_slot);
        }
        {
            self.builder.switch_to_block(ok);
        }
    }

    fn emit_branch(&mut self, target: u32, link: bool, mode: IntCC, rs: usize, rt: usize) {
        if link {
            let link_addr = self.builder.ins().iconst(
                types::I32,
                i64::from(self.last_pc.wrapping_add(8).cast_signed()),
            );
            self.store_reg(Reg::General(Cpu::DEFAULT_LINK_REG), link_addr);
        }

        let rs = self.load_reg(Reg::General(rs));
        let cmp = if rt != 0 {
            let rt = self.load_reg(Reg::General(rt));
            self.builder.ins().icmp(mode, rs, rt)
        } else {
            self.builder.ins().icmp_imm(mode, rs, 0)
        };

        let ok = self.builder.create_block();
        let fail = self.builder.create_block();
        let done = self.builder.create_block();

        self.builder.ins().brif(cmp, ok, &[], fail, &[]);
        {
            self.builder.switch_to_block(ok);
            let target = self
                .builder
                .ins()
                .iconst(types::I32, i64::from(target.cast_signed()));
            // We unconditionally saved reg
            self.emit_jump(target, None);
            self.builder.ins().jump(done, &[]);
        }
        {
            self.builder.switch_to_block(fail);
            self.builder.ins().jump(done, &[]);
        }
        self.builder.switch_to_block(done);
    }

    fn emit_jump(&mut self, target: Value, link_reg: Option<usize>) {
        if let Some(link_reg) = link_reg
            && link_reg != 0
        {
            let link_addr = self.builder.ins().iconst(
                types::I32,
                i64::from(self.last_pc.wrapping_add(8).cast_signed()),
            );
            self.store_reg(Reg::General(link_reg), link_addr);
        }

        self.builder.ins().store(
            MemFlags::new(),
            target,
            self.res_ptr,
            offset_of!(FuncResult, jump_addr).cast_signed() as i32,
        );
        let result = self.builder.ins().iconst(
            types::I32,
            i64::from((ExecutionResult::Jump as u32).cast_signed()),
        );
        self.builder.ins().store(
            MemFlags::new(),
            result,
            self.res_ptr,
            offset_of!(FuncResult, result).cast_signed() as i32,
        );
    }

    fn emit_return(
        &mut self,
        err: Option<ExecutionResult>,
        bad_vaddr: Option<u32>,
        load_delay_slot: Option<(Value, Value)>,
    ) {
        if let Some(load_delay_slot) = load_delay_slot {
            self.commit_load_delay(load_delay_slot);
        }

        if let Some(bad_vaddr) = bad_vaddr {
            let bad_vaddr = self
                .builder
                .ins()
                .iconst(types::I32, i64::from(bad_vaddr.cast_signed()));
            self.builder.ins().store(
                MemFlags::new(),
                bad_vaddr,
                self.res_ptr,
                offset_of!(FuncResult, bad_vaddr).cast_signed() as i32,
            );
        }

        let pc = self
            .builder
            .ins()
            .iconst(types::I32, i64::from(self.last_pc));
        self.builder.ins().store(
            MemFlags::new(),
            pc,
            self.res_ptr,
            offset_of!(FuncResult, last_pc).cast_signed() as i32,
        );

        let in_delay_slot = self
            .builder
            .ins()
            .iconst(types::I32, i64::from(self.last_in_delay_slot));
        self.builder.ins().store(
            MemFlags::new(),
            in_delay_slot,
            self.res_ptr,
            offset_of!(FuncResult, last_in_delay_slot).cast_signed() as i32,
        );

        // Force error set
        if let Some(err) = err {
            let result = self
                .builder
                .ins()
                .iconst(types::I32, (err as u64).cast_signed());
            self.builder.ins().store(
                MemFlags::new(),
                result,
                self.res_ptr,
                offset_of!(FuncResult, result).cast_signed() as i32,
            );
        }

        // Bye!
        self.builder.ins().return_(&[]);
    }

    fn commit_load_delay(&mut self, (load_delay_dest, load_delay_val): (Value, Value)) {
        let ptr_ty = self.module.target_config().pointer_type();

        let success = self
            .builder
            .ins()
            .icmp_imm(IntCC::NotEqual, load_delay_dest, 0);

        let nonzero = self.builder.create_block();
        let zero = self.builder.create_block();
        let done = self.builder.create_block();

        // In case load_mem fails, so we should skip write to zero reg
        self.builder.ins().brif(success, nonzero, &[], zero, &[]);
        {
            self.builder.switch_to_block(nonzero);
            let addr = {
                let offset1 = self
                    .builder
                    .ins()
                    .iconst(ptr_ty, offset_of!(Cpu, gpr).cast_signed() as i64);
                let offset2 = self.builder.ins().imul_imm(load_delay_dest, 4);
                let offset = self.builder.ins().iadd(offset1, offset2);

                self.builder.ins().iadd(self.cpu_ptr, offset)
            };
            self.builder
                .ins()
                .store(MemFlags::new(), load_delay_val, addr, 0);
            self.builder.ins().jump(done, &[]);
        }
        {
            self.builder.switch_to_block(zero);
            self.builder.ins().jump(done, &[]);
        }
        self.builder.switch_to_block(done);

        if !self.pending_load_delay_gen {
            let dest = self.builder.ins().iconst(ptr_ty, 0);
            let value = self.builder.ins().iconst(types::I32, 0);
            self.set_pending_load(dest, value);
        }
    }

    fn read_pending_load(&mut self) -> (Value, Value) {
        let ptr_ty = self.module.target_config().pointer_type();

        let load_delay_dest = self.builder.ins().load(
            ptr_ty,
            MemFlags::new(),
            self.cpu_ptr,
            offset_of!(Cpu, pending_load.dest).cast_signed() as i32,
        );
        let load_delay_val = self.builder.ins().load(
            types::I32,
            MemFlags::new(),
            self.cpu_ptr,
            offset_of!(Cpu, pending_load.value).cast_signed() as i32,
        );

        (load_delay_dest, load_delay_val)
    }

    fn set_pending_load(&mut self, dest: Value, value: Value) {
        self.builder.ins().store(
            MemFlags::new(),
            dest,
            self.cpu_ptr,
            offset_of!(Cpu, pending_load.dest).cast_signed() as i32,
        );
        self.builder.ins().store(
            MemFlags::new(),
            value,
            self.cpu_ptr,
            offset_of!(Cpu, pending_load.value).cast_signed() as i32,
        );
    }

    fn load_reg(&mut self, reg: Reg) -> Value {
        self.builder.ins().load(
            types::I32,
            MemFlags::new(),
            self.cpu_ptr,
            reg.byte_offset().cast_signed() as i32,
        )
    }

    fn store_reg(&mut self, reg: Reg, val: Value) {
        self.builder.ins().store(
            MemFlags::new(),
            val,
            self.cpu_ptr,
            reg.byte_offset().cast_signed() as i32,
        );
    }
}

#[derive(Copy, Clone)]
enum Reg {
    Hi,
    Lo,
    General(usize),
    Cop0(usize),
}

impl Reg {
    fn byte_offset(self) -> usize {
        match self {
            Self::Hi => offset_of!(Cpu, hi),
            Self::Lo => offset_of!(Cpu, lo),
            Self::General(idx) => offset_of!(Cpu, gpr) + 4 * idx,
            Self::Cop0(idx) => offset_of!(Cpu, cop0.regs) + 4 * idx,
        }
    }
}
