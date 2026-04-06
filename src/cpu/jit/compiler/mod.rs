use std::mem;

use cranelift::{
    codegen::Context,
    prelude::{AbiParam, FunctionBuilder, FunctionBuilderContext, Signature, Value, types},
};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};

use super::{
    FuncPtr,
    decoder::{DecRes, InsIter},
};

mod codegen;
mod stubs;

pub struct JitCtx {
    /// Module where all compiled functions reside
    module: JITModule,
    /// Cache for a builder of a function
    fn_ctx: FunctionBuilderContext,
    /// Cache for a context of a function
    ctx: Context,
    /// Cache for a signature of a function
    sig: Signature,
    stubs: Stubs,
}

pub struct FnCtx<'local, 'builder> {
    pub module: &'local mut JITModule,
    pub builder: &'local mut FunctionBuilder<'builder>,
    pub res_ptr: Value,
    pub ctx_ptr: Value,
    pub cpu_ptr: Value,
    pub bus_ptr: Value,
    pub count: u64,
    pub last_pc: u32,
    pub last_in_delay_slot: bool,
    pub stubs: Stubs,
}

#[derive(Copy, Clone)]
pub struct Stubs {
    pub bus_store_name: FuncId,
    pub bus_load_name: FuncId,
}

impl Default for JitCtx {
    fn default() -> Self {
        let mut builder = JITBuilder::new(cranelift_module::default_libcall_names()).unwrap();
        builder
            .symbol("bus_store", stubs::bus_store as *const u8)
            .symbol("bus_load", stubs::bus_load as *const u8);

        let mut module = JITModule::new(builder);

        let fn_ctx = FunctionBuilderContext::new();
        let ctx = module.make_context();
        let mut sig = module.make_signature();
        let ptr_ty = module.target_config().pointer_type();

        let bus_store_name = {
            module.clear_signature(&mut sig);
            sig.params.push(AbiParam::new(ptr_ty));
            sig.params.push(AbiParam::new(ptr_ty));
            sig.params.push(AbiParam::new(ptr_ty));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I8));
            sig.params.push(AbiParam::new(types::I8));
            sig.returns.push(AbiParam::new(types::I32));
            module
                .declare_function("bus_store", Linkage::Import, &sig)
                .unwrap()
        };
        let bus_load_name = {
            module.clear_signature(&mut sig);
            sig.params.push(AbiParam::new(ptr_ty));
            sig.params.push(AbiParam::new(ptr_ty));
            sig.params.push(AbiParam::new(ptr_ty));
            sig.params.push(AbiParam::new(types::I8));
            sig.params.push(AbiParam::new(types::I32));
            sig.params.push(AbiParam::new(types::I8));
            sig.returns.push(AbiParam::new(types::I32));
            module
                .declare_function("bus_load", Linkage::Import, &sig)
                .unwrap()
        };

        Self {
            module,
            fn_ctx,
            ctx,
            sig,
            stubs: Stubs {
                bus_store_name,
                bus_load_name,
            },
        }
    }
}

impl JitCtx {
    pub fn make_new_fn(&mut self, enter_pc: u32, decs: InsIter<'_>) -> FuncPtr {
        let ptr_ty = self.module.target_config().pointer_type();

        self.module.clear_signature(&mut self.sig);
        self.sig.params.push(AbiParam::new(ptr_ty)); // *mut res
        self.sig.params.push(AbiParam::new(ptr_ty)); // *mut ctx
        self.sig.params.push(AbiParam::new(ptr_ty)); // *mut cpu
        self.sig.params.push(AbiParam::new(ptr_ty)); // *mut bus

        let fn_name = self
            .module
            .declare_function(&format!("enter_{enter_pc:#}"), Linkage::Local, &self.sig)
            .unwrap();

        let mut b = FunctionBuilder::new(&mut self.ctx.func, &mut self.fn_ctx);

        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        let res_ptr = b.block_params(entry)[0];
        let ctx_ptr = b.block_params(entry)[1];
        let cpu_ptr = b.block_params(entry)[2];
        let bus_ptr = b.block_params(entry)[3];

        let mut fn_ctx = FnCtx {
            res_ptr,
            ctx_ptr,
            cpu_ptr,
            bus_ptr,
            count: 0,
            last_pc: enter_pc,
            last_in_delay_slot: false,
            module: &mut self.module,
            builder: &mut b,
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
