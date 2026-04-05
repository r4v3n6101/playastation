use std::mem;

use cranelift::{
    codegen::Context,
    prelude::{AbiParam, FunctionBuilder, FunctionBuilderContext},
};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};

use super::{FuncPtr, decoder::InsIter};

mod codegen;

pub struct JitCtx {
    /// Module where all compiled functions reside
    module: JITModule,
    /// Cache for a builder of a function
    fn_ctx: FunctionBuilderContext,
    /// Cache for a context of a function
    ctx: Context,
}

impl Default for JitCtx {
    fn default() -> Self {
        let module =
            JITModule::new(JITBuilder::new(cranelift_module::default_libcall_names()).unwrap());
        let fn_ctx = FunctionBuilderContext::new();
        let ctx = module.make_context();

        Self {
            module,
            fn_ctx,
            ctx,
        }
    }
}

impl JitCtx {
    pub fn make_new_fn(&mut self, enter_pc: u32, decs: InsIter<'_>) -> FuncPtr {
        let ptr_ty = self.module.target_config().pointer_type();

        let mut sig = self.module.make_signature();
        sig.params.push(AbiParam::new(ptr_ty)); // *mut res
        sig.params.push(AbiParam::new(ptr_ty)); // *mut ctx
        sig.params.push(AbiParam::new(ptr_ty)); // *mut cpu
        sig.params.push(AbiParam::new(ptr_ty)); // *mut bus

        let fn_name = self
            .module
            .declare_function(&format!("enter_{enter_pc:#}"), Linkage::Local, &sig)
            .unwrap();

        let mut b = FunctionBuilder::new(&mut self.ctx.func, &mut self.fn_ctx);

        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        let res_ptr = b.block_params(entry)[0];
        let ctx_ptr = b.block_params(entry)[1];
        let cpu_ptr = b.block_params(entry)[2];
        let bus_ptr = b.block_params(entry)[3];

        let mut count = 0u64;
        decs.for_each(|decoded| {
            codegen::emit_op(
                &mut b, &mut count, res_ptr, ctx_ptr, cpu_ptr, bus_ptr, &decoded,
            );
        });

        b.seal_all_blocks();
        b.finalize();

        self.module.define_function(fn_name, &mut self.ctx).unwrap();
        self.module.clear_context(&mut self.ctx);
        self.module.finalize_definitions().unwrap();

        // Safety: it's a program bug if transmute is invalid
        unsafe { mem::transmute(self.module.get_finalized_function(fn_name)) }
    }
}
