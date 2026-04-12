use std::{mem, ptr};

use cranelift::{
    codegen::Context,
    jit::{JITBuilder, JITModule},
    module::{Module, default_libcall_names},
    prelude::FunctionBuilderContext,
};

use crate::{cpu::Cpu, interconnect::Bus};

use super::{ExecutionResult, Executor, decoder::Operation};

mod codegen;
mod stubs;
#[cfg(test)]
mod tests;

type FnPtr = fn(*mut ExecutionResult, *mut Cpu, *mut Bus);

pub struct Jit {
    compiler: CompilerCtx,
}

struct CompilerCtx {
    /// Module where all compiled functions reside
    module: JITModule,
    /// Cache for a context
    ctx: Context,
    /// Cache for a builder
    fn_build_ctx: FunctionBuilderContext,
}

impl Default for Jit {
    fn default() -> Self {
        let fn_builder = JITBuilder::new(default_libcall_names()).unwrap();
        let module = JITModule::new(fn_builder);

        let fn_build_ctx = FunctionBuilderContext::new();
        let ctx = module.make_context();

        Self {
            compiler: CompilerCtx {
                module,
                ctx,
                fn_build_ctx,
            },
        }
    }
}

impl Executor for Jit {
    fn run(&mut self, ins_block: &[Operation], cpu: &mut Cpu, bus: &mut Bus) -> ExecutionResult {
        let mut result = ExecutionResult {
            last_pc: cpu.pc,
            last_in_delay_slot: false,
            exception: None,
        };

        let fn_name = {
            self.compiler.module.clear_context(&mut self.compiler.ctx);
            let mut fn_ctx =
                codegen::FnCtx::create_and_emit_header(&mut self.compiler, result.last_pc);
            for ins in ins_block {
                match *ins {
                    Operation::Instruction {
                        pc,
                        in_delay_slot,
                        ins,
                        op,
                    } => {
                        fn_ctx.emit_op(pc, in_delay_slot, ins, op);
                    }
                    Operation::Break {
                        pc,
                        in_delay_slot,
                        cause,
                    } => {
                        fn_ctx.emit_exception(pc, in_delay_slot, cause);
                    }
                }
            }

            fn_ctx.emit_trailer();
            fn_ctx.finalize()
        };

        self.compiler
            .module
            .define_function(fn_name, &mut self.compiler.ctx)
            .unwrap();
        self.compiler.module.finalize_definitions().unwrap();

        let fnptr = self.compiler.module.get_finalized_function(fn_name);

        // Safety: compiled with suchs signature
        let func: FnPtr = unsafe { mem::transmute(fnptr) };

        func(
            ptr::from_mut(&mut result),
            ptr::from_mut(cpu),
            ptr::from_mut(bus),
        );

        result
    }
}
