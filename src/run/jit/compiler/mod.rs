use std::mem;

use cranelift::{
    codegen::Context,
    jit::{JITBuilder, JITModule},
    module::{DataDescription, Linkage, Module, default_libcall_names},
    prelude::FunctionBuilderContext,
};

use super::{
    FuncPtr,
    decoder::{DecRes, InsIter},
};

mod codegen;
mod stubs;
#[cfg(test)]
mod tests;

pub struct ModCtx {
    /// Module where all compiled functions reside
    module: JITModule,
    /// Cache for a context
    ctx: Context,
    /// Cache for a builder
    fn_build_ctx: FunctionBuilderContext,
    /// Whether to generate load-delay store
    pending_load_delay_gen: bool,
}

impl Default for ModCtx {
    fn default() -> Self {
        let fn_builder = JITBuilder::new(default_libcall_names()).unwrap();
        let module = JITModule::new(fn_builder);

        let fn_build_ctx = FunctionBuilderContext::new();
        let ctx = module.make_context();

        Self {
            module,
            ctx,
            fn_build_ctx,
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
