use std::mem;

use cranelift::{
    codegen::Context,
    jit::{JITBuilder, JITModule},
    module::{DataDescription, DataId, Linkage, Module, default_libcall_names},
    prelude::FunctionBuilderContext,
};

use super::{
    FuncPtr,
    decoder::{DecRes, InsIter},
};

mod codegen;
mod stubs;

pub struct ModCtx {
    /// Module where all compiled functions reside
    module: JITModule,
    /// Cache for a context
    ctx: Context,
    /// Cache for a builder
    fn_build_ctx: FunctionBuilderContext,
    /// Whether to generate load-delay store
    pending_load_delay_gen: bool,
    /// Global pending register where to store load-delay slot
    load_delay_dest: DataId,
    /// Global slot for load-delay value
    load_delay_val: DataId,
}

impl Default for ModCtx {
    fn default() -> Self {
        let fn_builder = JITBuilder::new(default_libcall_names()).unwrap();
        let mut module = JITModule::new(fn_builder);

        let fn_build_ctx = FunctionBuilderContext::new();
        let ctx = module.make_context();

        let global_value_fn = |module: &mut JITModule, name, size| {
            let gv = module
                .declare_data(name, Linkage::Local, true, false)
                .unwrap();
            let mut data = DataDescription::new();
            data.define_zeroinit(size);
            module.define_data(gv, &data).unwrap();

            gv
        };

        let load_delay_dest = global_value_fn(&mut module, "load_delay_dest", 1);
        let load_delay_val = global_value_fn(&mut module, "load_delay_val", 4);

        Self {
            module,
            ctx,
            fn_build_ctx,
            load_delay_dest,
            load_delay_val,
            pending_load_delay_gen: false,
        }
    }
}

impl ModCtx {
    pub fn make_new_fn(&mut self, enter_pc: u32, decs: InsIter<'_>) -> FuncPtr {
        let fn_name = {
            let mut fn_ctx = codegen::FnCtx::create_and_emit_header(self, enter_pc);
            decs.for_each(|decoded| match decoded {
                DecRes::Decoded {
                    pc,
                    ins,
                    in_delay_slot,
                    op,
                } => {
                    fn_ctx.emit_op(pc, in_delay_slot, ins, op);
                }
                DecRes::Exception {
                    pc,
                    in_delay_slot,
                    exc,
                } => {
                    fn_ctx.emit_exception(pc, in_delay_slot, exc);
                }
            });

            fn_ctx.emit_trailer();
            fn_ctx.finalize()
        };

        self.module.define_function(fn_name, &mut self.ctx).unwrap();
        self.module.clear_context(&mut self.ctx);
        self.module.finalize_definitions().unwrap();

        // Safety: it's a program bug if transmute is invalid
        unsafe { mem::transmute(self.module.get_finalized_function(fn_name)) }
    }
}

#[cfg(test)]
mod tests {
    use crate::{cpu::Cpu, interconnect::Bus};

    use super::{
        super::{ExecutionResult, FuncResult, decoder::InsIter},
        ModCtx,
    };

    fn compile_and_run(
        ctx: &mut ModCtx,
        enter_pc: u32,
        words: &[(u32, u32)],
        cpu: &mut Cpu,
        bus: &mut Bus,
    ) -> FuncResult {
        words.iter().for_each(|&(addr, val)| {
            let _ = bus.store(addr, val.to_le_bytes());
        });

        let mut pc = enter_pc;
        let func = ctx.make_new_fn(enter_pc, InsIter::new_start_from(&mut pc, bus, words.len()));

        let mut res = Default::default();
        func(&mut res, cpu, bus);
        res
    }

    #[test]
    fn compiles_and_executes_alu_block() {
        let mut ctx = ModCtx::default();
        let mut cpu = Cpu::default();
        let mut bus = Bus::default();

        let res = compile_and_run(
            &mut ctx,
            0,
            &[
                (0x0000_0000, 0x2408_0005), // addiu t0, zero, 5
                (0x0000_0004, 0x2509_0007), // addiu t1, t0, 7
                (0x0000_0008, 0x0109_5021), // addu  t2, t0, t1
                (0x0000_000C, 0x2400_0001), // addiu zero, zero, 1
            ],
            &mut cpu,
            &mut bus,
        );

        assert_eq!(cpu.regs.general[8], 5);
        assert_eq!(cpu.regs.general[9], 12);
        assert_eq!(cpu.regs.general[10], 17);
        assert_eq!(cpu.regs.general[0], 0);

        assert_eq!(
            res,
            FuncResult {
                result: ExecutionResult::Success,
                last_pc: 0x0000_000C,
                last_in_delay_slot: 0,
                bad_vaddr: 0
            }
        );
    }

    #[test]
    fn stops_on_overflow_and_preserves_destination_register() {
        let mut ctx = ModCtx::default();
        let mut cpu = Cpu::default();
        let mut bus = Bus::default();

        let res = compile_and_run(
            &mut ctx,
            0,
            &[
                (0x0000_0000, 0x3C08_7FFF), // lui   t0, 0x7fff
                (0x0000_0004, 0x3508_FFFF), // ori   t0, t0, 0xffff
                (0x0000_0008, 0x2108_0001), // addi  t0, t0, 1
                (0x0000_000C, 0x2409_0001), // addiu t1, zero, 1
            ],
            &mut cpu,
            &mut bus,
        );

        assert_eq!(cpu.regs.general[8], 0x7FFF_FFFF);
        assert_eq!(cpu.regs.general[9], 0);

        assert_eq!(
            res,
            FuncResult {
                result: ExecutionResult::Overflow,
                last_pc: 0x0000_0008,
                last_in_delay_slot: 0,
                bad_vaddr: 0
            }
        );
    }

    #[test]
    fn applies_load_delay_and_handles_nested_loads() {
        let mut ctx = ModCtx::default();
        let mut cpu = Cpu::default();
        let mut bus = Bus::default();

        bus.store(0x20, 0x1111_1111u32.to_le_bytes()).unwrap();
        bus.store(0x24, 0x2222_2222u32.to_le_bytes()).unwrap();

        let res = compile_and_run(
            &mut ctx,
            0,
            &[
                (0x0000_0000, 0x2408_0020), // addiu t0, zero, 0x20
                (0x0000_0004, 0x8D09_0000), // lw    t1, 0(t0)
                (0x0000_0008, 0x0120_5021), // addu  t2, t1, zero
                (0x0000_000C, 0x8D09_0004), // lw    t1, 4(t0)
                (0x0000_0010, 0x0120_5821), // addu  t3, t1, zero
                (0x0000_0014, 0x0120_6021), // addu  t4, t1, zero
            ],
            &mut cpu,
            &mut bus,
        );

        // First dependent instruction must still see the old t1 value.
        assert_eq!(cpu.regs.general[10], 0);
        // After the first delay slot, the first load becomes visible.
        assert_eq!(cpu.regs.general[11], 0x1111_1111);
        // After the nested load delay slot, the second load becomes visible.
        assert_eq!(cpu.regs.general[9], 0x2222_2222);
        assert_eq!(cpu.regs.general[12], 0x2222_2222);

        assert_eq!(
            res,
            FuncResult {
                result: ExecutionResult::Success,
                last_pc: 0x0000_0014,
                last_in_delay_slot: 0,
                bad_vaddr: 0
            }
        );
    }

    #[test]
    fn second_load_uses_old_base_when_it_depends_on_previous_load() {
        let mut ctx = ModCtx::default();
        let mut cpu = Cpu::default();
        let mut bus = Bus::default();

        bus.store(0x40, 0x1111_1111u32.to_le_bytes()).unwrap();
        bus.store(0x20, 0x0000_0040u32.to_le_bytes()).unwrap();
        bus.store(0x30, 0x2222_2222u32.to_le_bytes()).unwrap();

        let res = compile_and_run(
            &mut ctx,
            0,
            &[
                (0x0000_0000, 0x2408_0020), // addiu t0, zero, 0x20
                (0x0000_0004, 0x8D09_0000), // lw    t1, 0(t0)
                (0x0000_0008, 0x8D2A_0000), // lw    t2, 0(t1)
                (0x0000_000C, 0x0120_5821), // addu  t3, t1, zero
                (0x0000_0010, 0x0140_6021), // addu  t4, t2, zero
            ],
            &mut cpu,
            &mut bus,
        );

        // The first load becomes visible only after the second load has already
        // computed its address, so the nested load must use the old t1 value (0).
        assert_eq!(cpu.regs.general[8], 0x0000_0020); // t0
        assert_eq!(cpu.regs.general[9], 0x0000_0040); // t1
        assert_eq!(cpu.regs.general[10], 0x2408_0020); // t2
        assert_eq!(cpu.regs.general[11], 0x0000_0040); // t3
        assert_eq!(cpu.regs.general[12], 0x2408_0020); // t4

        assert_eq!(
            res,
            FuncResult {
                result: ExecutionResult::Success,
                last_pc: 0x0000_0010,
                last_in_delay_slot: 0,
                bad_vaddr: 0
            }
        );
    }

    #[test]
    fn moves_values_through_hi_lo_and_executes_mult_and_div() {
        let mut ctx = ModCtx::default();
        let mut cpu = Cpu::default();
        let mut bus = Bus::default();

        let res = compile_and_run(
            &mut ctx,
            0,
            &[
                (0x0000_0000, 0x2408_0006), // addiu t0, zero, 6
                (0x0000_0004, 0x2409_0003), // addiu t1, zero, 3
                (0x0000_0008, 0x0100_0011), // mthi  t0
                (0x0000_000C, 0x0120_0013), // mtlo  t1
                (0x0000_0010, 0x0000_8010), // mfhi  s0
                (0x0000_0014, 0x0000_8812), // mflo  s1
                (0x0000_0018, 0x0109_0018), // mult  t0, t1
                (0x0000_001C, 0x0000_9010), // mfhi  s2
                (0x0000_0020, 0x0000_9812), // mflo  s3
                (0x0000_0024, 0x2408_FFF9), // addiu t0, zero, -7
                (0x0000_0028, 0x0109_001A), // div   t0, t1
                (0x0000_002C, 0x0000_A010), // mfhi  s4
                (0x0000_0030, 0x0000_A812), // mflo  s5
            ],
            &mut cpu,
            &mut bus,
        );

        assert_eq!(cpu.regs.hi, 0xFFFF_FFFF);
        assert_eq!(cpu.regs.lo, 0xFFFF_FFFE);
        assert_eq!(cpu.regs.general[16], 6);
        assert_eq!(cpu.regs.general[17], 3);
        assert_eq!(cpu.regs.general[18], 0);
        assert_eq!(cpu.regs.general[19], 18);
        assert_eq!(cpu.regs.general[20], 0xFFFF_FFFF);
        assert_eq!(cpu.regs.general[21], 0xFFFF_FFFE);

        assert_eq!(
            res,
            FuncResult {
                result: ExecutionResult::Success,
                last_pc: 0x0000_0030,
                last_in_delay_slot: 0,
                bad_vaddr: 0
            }
        );
    }

    #[test]
    fn moves_values_between_gpr_and_cop0_with_load_delay() {
        let mut ctx = ModCtx::default();
        let mut cpu = Cpu::default();
        let mut bus = Bus::default();

        let res = compile_and_run(
            &mut ctx,
            0,
            &[
                (0x0000_0000, 0x2408_1234), // addiu t0, zero, 0x1234
                (0x0000_0004, 0x4088_7800), // mtc0  t0, $15
                (0x0000_0008, 0x4009_7800), // mfc0  t1, $15
                (0x0000_000C, 0x0120_5021), // addu  t2, t1, zero
                (0x0000_0010, 0x0120_5821), // addu  t3, t1, zero
            ],
            &mut cpu,
            &mut bus,
        );

        assert_eq!(cpu.cop0.regs[15], 0x0000_1234);
        assert_eq!(cpu.regs.general[9], 0x0000_1234);
        assert_eq!(cpu.regs.general[10], 0);
        assert_eq!(cpu.regs.general[11], 0x0000_1234);

        assert_eq!(
            res,
            FuncResult {
                result: ExecutionResult::Success,
                last_pc: 0x0000_0010,
                last_in_delay_slot: 0,
                bad_vaddr: 0
            }
        );
    }
}
