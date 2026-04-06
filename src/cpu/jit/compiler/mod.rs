use std::mem;

use cranelift::{
    codegen::Context,
    jit::{JITBuilder, JITModule},
    module::{FuncId, Linkage, Module, default_libcall_names},
    prelude::{AbiParam, FunctionBuilder, FunctionBuilderContext, Value, types},
};

use super::{
    FuncPtr,
    decoder::{DecRes, InsIter},
};

mod codegen;
mod stubs;

pub struct ModuleCtx {
    /// Module where all compiled functions reside
    module: JITModule,
    /// Cache for a builder of a function
    fn_ctx: FunctionBuilderContext,
    /// Cache for a context of a function
    ctx: Context,
    /// Imported extern "C" functions
    stubs: Stubs,
    /// Carried load delay slot for pending writes (probably cross boundary).
    /// If dest=0 then load slot is empty or reg0's write is discarded
    load_delay: (u8, u32),
}

struct FnCtx<'module, 'func> {
    module: &'module mut JITModule,
    builder: &'module mut FunctionBuilder<'func>,
    load_delay: &'module mut (u8, u32),
    res_ptr: Value,
    cpu_ptr: Value,
    bus_ptr: Value,
    last_pc: u32,
    last_in_delay_slot: bool,
    stubs: Stubs,
}

#[derive(Copy, Clone)]
struct Stubs {
    bus_load_name: FuncId,
    bus_store_name: FuncId,
}

impl Default for ModuleCtx {
    fn default() -> Self {
        let mut builder = JITBuilder::new(default_libcall_names()).unwrap();
        builder
            .symbol("bus_store", stubs::bus_store as *const u8)
            .symbol("bus_load", stubs::bus_load as *const u8);

        let mut module = JITModule::new(builder);

        let fn_ctx = FunctionBuilderContext::new();
        let ctx = module.make_context();
        let mut sig = module.make_signature();
        let ptr_ty = module.target_config().pointer_type();

        let bus_load_name = {
            module.clear_signature(&mut sig);
            sig.params.push(AbiParam::new(ptr_ty));
            sig.params.push(AbiParam::new(ptr_ty));
            sig.params.push(AbiParam::new(ptr_ty));
            sig.params.push(AbiParam::new(ptr_ty));
            sig.params.push(AbiParam::new(types::I8));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I8));
            sig.params.push(AbiParam::new(types::I8));
            sig.params.push(AbiParam::new(types::I8));
            sig.returns.push(AbiParam::new(types::I8));
            module
                .declare_function("bus_load", Linkage::Import, &sig)
                .unwrap()
        };
        let bus_store_name = {
            module.clear_signature(&mut sig);
            sig.params.push(AbiParam::new(ptr_ty));
            sig.params.push(AbiParam::new(ptr_ty));
            sig.params.push(AbiParam::new(ptr_ty));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I8));
            sig.params.push(AbiParam::new(types::I8));
            sig.returns.push(AbiParam::new(types::I8));
            module
                .declare_function("bus_store", Linkage::Import, &sig)
                .unwrap()
        };

        Self {
            module,
            fn_ctx,
            ctx,
            load_delay: (0, 0),
            stubs: Stubs {
                bus_load_name,
                bus_store_name,
            },
        }
    }
}

impl ModuleCtx {
    pub fn make_new_fn(&mut self, enter_pc: u32, decs: InsIter<'_>) -> FuncPtr {
        let ptr_ty = self.module.target_config().pointer_type();

        let mut sig = self.module.make_signature();
        sig.params.push(AbiParam::new(ptr_ty)); // *mut res
        sig.params.push(AbiParam::new(ptr_ty)); // *mut cpu
        sig.params.push(AbiParam::new(ptr_ty)); // *mut bus

        let fn_name = self
            .module
            .declare_function(&format!("enter_{enter_pc:#}"), Linkage::Local, &sig)
            .unwrap();

        let mut b = FunctionBuilder::new(&mut self.ctx.func, &mut self.fn_ctx);
        b.func.signature = sig;

        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        let res_ptr = b.block_params(entry)[0];
        let cpu_ptr = b.block_params(entry)[1];
        let bus_ptr = b.block_params(entry)[2];

        let mut fn_ctx = FnCtx {
            res_ptr,
            cpu_ptr,
            bus_ptr,
            builder: &mut b,
            module: &mut self.module,
            load_delay: &mut self.load_delay,
            last_pc: 0,
            last_in_delay_slot: false,
            stubs: self.stubs,
        };
        decs.for_each(|decoded| match decoded {
            DecRes::Decoded {
                pc,
                ins,
                in_delay_slot,
                op,
            } => {
                fn_ctx.last_pc = pc;
                fn_ctx.last_in_delay_slot = in_delay_slot;
                codegen::emit_op(&mut fn_ctx, ins, op);
            }
            DecRes::Exception {
                pc,
                in_delay_slot,
                exc,
            } => {
                fn_ctx.last_pc = pc;
                fn_ctx.last_in_delay_slot = in_delay_slot;
                // TODO
            }
        });

        codegen::emit_trailer(&mut fn_ctx);

        b.seal_all_blocks();
        b.finalize();

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
        ModuleCtx,
    };

    fn compile_and_run(
        ctx: &mut ModuleCtx,
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
        let mut ctx = ModuleCtx::default();
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
                loads: 0,
                bad_vaddr: 0
            }
        );
    }

    #[test]
    fn stops_on_overflow_and_preserves_destination_register() {
        let mut ctx = ModuleCtx::default();
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
                loads: 0,
                bad_vaddr: 0
            }
        );
    }

    #[test]
    fn stores_to_bus_and_carries_pending_load_delay() {
        let mut ctx = ModuleCtx::default();
        let mut cpu = Cpu::default();
        let mut bus = Bus::default();

        let res = compile_and_run(
            &mut ctx,
            0,
            &[
                (0x0000_0000, 0x2408_0010), // addiu t0, zero, 0x10
                (0x0000_0004, 0x2409_1234), // addiu t1, zero, 0x1234
                (0x0000_0008, 0xAD09_0000), // sw    t1, 0(t0)
                (0x0000_000C, 0x8D0A_0000), // lw    t2, 0(t0)
                (0x0000_0010, 0x8D0A_0000), // lw    t2, 0(t0)
                (0x0000_0014, 0x8D0A_0000), // lw    t2, 0(t0)
                (0x0000_0018, 0x8D0A_0000), // lw    t2, 0(t0)
                (0x0000_001C, 0x8D0A_0000), // lw    t2, 0(t0)
            ],
            &mut cpu,
            &mut bus,
        );

        assert_eq!(u32::from_le_bytes(bus.load(0x10).unwrap()), 0x0000_1234);
        assert_eq!(cpu.regs.general[10], 0);
        assert_eq!(ctx.load_delay, (10, 0x0000_1234));

        assert_eq!(
            res,
            FuncResult {
                result: ExecutionResult::Success,
                last_pc: 0x0000_001C,
                last_in_delay_slot: 0,
                loads: 5,
                bad_vaddr: 0
            }
        );
    }
}
