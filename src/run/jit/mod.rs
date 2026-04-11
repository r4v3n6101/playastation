use std::mem;

use cranelift::{
    codegen::Context,
    jit::{JITBuilder, JITModule},
    module::{Module, default_libcall_names},
    prelude::FunctionBuilderContext,
};

use crate::{cpu::Cpu, interconnect::Bus};

mod codegen;
mod stubs;
#[cfg(test)]
mod tests;

type FnPtr = fn(*mut FuncResult, *mut Cpu, *mut Bus);

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct FuncResult {
    /// Result of function execution.
    result: ExecutionResult,
    /// PC of last executed instruction.
    last_pc: u32,
    /// Flag (0=false, 1=true) whether last executed instruction is in delay slot.
    last_in_delay_slot: u32,

    // Ideally this should be inside [`ExecutionResult`]
    /// Filled in case of invalid memory ops (unaligned load/store, unmapped).
    bad_vaddr: u32,
    /// PC being jumped to.
    jump_addr: u32,
}

// TODO : use normal enum, just like in Rust
#[repr(u32)]
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub enum ExecutionResult {
    #[default]
    Success = 0,
    Jump = 1,
    Overflow = 2,
    UnalignedLoad = 3,
    UnalignedStore = 4,
    ReservedInstruction = 5,
    InstructionBus = 6,
    DataBus = 7,
    Syscall = 8,
    Break = 9,
}

pub struct Storage {
    /// Module where all compiled functions reside
    module: JITModule,
    /// Cache for a context
    ctx: Context,
    /// Cache for a builder
    fn_build_ctx: FunctionBuilderContext,
}

impl Default for Storage {
    fn default() -> Self {
        let fn_builder = JITBuilder::new(default_libcall_names()).unwrap();
        let module = JITModule::new(fn_builder);

        let fn_build_ctx = FunctionBuilderContext::new();
        let ctx = module.make_context();

        Self {
            module,
            ctx,
            fn_build_ctx,
        }
    }
}

pub fn compile_fn<'a>(storage: &mut Storage, enter_pc: u32, decs: InsIter<'_>) -> FnPtr {
    let fn_name = {
        let mut fn_ctx = codegen::FnCtx::create_and_emit_header(storage, enter_pc);
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

    storage
        .module
        .define_function(fn_name, &mut storage.ctx)
        .unwrap();
    storage.module.clear_context(&mut storage.ctx);
    storage.module.finalize_definitions().unwrap();

    let fnptr = storage.module.get_finalized_function(fn_name);

    // Safety: it was compiled with such type assumption
    unsafe { mem::transmute(fnptr) }
}
