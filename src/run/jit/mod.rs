use std::{cell::UnsafeCell, marker::PhantomData, mem, ptr};

use cranelift::{
    codegen::Context,
    jit::{JITBuilder, JITModule},
    module::{Module, default_libcall_names},
    prelude::FunctionBuilderContext,
};

use crate::{cpu::Cpu, interconnect::Bus};

use super::decoder::{DecRes, InsIter};

mod codegen;
mod stubs;
#[cfg(test)]
mod tests;

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

pub struct CompiledFunc<'a> {
    /// Pointer to compiled function
    fnptr: *const u8,
    /// Storage of compiled function w
    _storage: PhantomData<&'a Storage>,
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

impl CompiledFunc<'_> {
    pub fn call(&self, res: &mut FuncResult, cpu: &mut Cpu, bus: &mut Bus) {
        // Safety: we've compiled function with such signature
        let fnptr: fn(*mut FuncResult, *mut Cpu, *mut Bus) = unsafe { mem::transmute(self.fnptr) };

        fnptr(ptr::from_mut(res), ptr::from_mut(cpu), ptr::from_mut(bus))
    }
}

/// Compile a function from iterator of instructions.
///
/// Safety: storage is used to bound [`CompiledFunc`] to LT of [`Storage`].
/// Be sure that the call is unique, otherwise double mutable access occurs.
pub unsafe fn compile_fn<'a>(
    storage: &'a UnsafeCell<Storage>,
    enter_pc: u32,
    decs: InsIter<'_>,
) -> CompiledFunc<'a> {
    let storage = unsafe { &mut *storage.get() };
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

    CompiledFunc {
        fnptr,
        _storage: PhantomData,
    }
}
