use std::mem;

use crate::{
    cpu::{Cpu, Exception},
    interconnect::Bus,
};

mod decoder;
pub mod interpreter;
// pub mod jit;

#[derive(Debug)]
pub struct CpuExecutor<E> {
    pub cpu: Cpu,
    pub executor: E,

    /// Maximum block size. If the last op is branch delay, block may be max+1
    pub block_size: usize,
    /// Cache of decoded block of ops
    block: Vec<decoder::Operation>,
}

pub trait Executor {
    fn run(
        &mut self,
        ins_block: &[decoder::Operation],
        cpu: &mut Cpu,
        bus: &mut Bus,
    ) -> ExecutionResult;
}

#[derive(Debug, Default, Copy, Clone)]
pub struct ExecutionResult {
    pub last_pc: u32,
    pub last_in_delay_slot: bool,
    pub exception: Option<Exception>,
}

impl<E> Default for CpuExecutor<E>
where
    E: Default,
{
    fn default() -> Self {
        const DEFAULT_INS_BLOCK: usize = 4096;

        Self {
            cpu: Cpu::default(),
            executor: E::default(),

            block_size: DEFAULT_INS_BLOCK,
            block: Vec::with_capacity(DEFAULT_INS_BLOCK + 1),
        }
    }
}

impl<E> CpuExecutor<E>
where
    E: Executor,
{
    pub fn run(&mut self, bus: &mut Bus) {
        decoder::decode_block(&mut self.block, &self.cpu, bus, self.block_size);
        let execution = self.executor.run(&self.block, &mut self.cpu, bus);

        self.cpu.cop0.set_hw_irq(bus.int_ctrl.pending());
        let interrupt = self
            .cpu
            .cop0
            .interrupt_pending()
            .then_some(ExecutionResult {
                last_pc: execution.last_pc,
                last_in_delay_slot: execution.last_in_delay_slot,
                exception: Some(Exception::Interrupt),
            });

        // Interrupt changes flow like it's an error in the last op
        let execution = interrupt.unwrap_or(execution);
        if let Some(exception) = execution.exception {
            self.cpu.cop0.exception_enter(
                exception,
                execution.last_pc,
                execution.last_in_delay_slot,
            );
            self.cpu.pc = self.cpu.cop0.exception_handler();

            // Commit, so exception handler could see last load
            let load_delay_slot = mem::take(&mut self.cpu.pending_load);
            self.cpu.gpr[load_delay_slot.dest] = load_delay_slot.value;
            self.cpu.gpr[0] = 0;
        } else {
            let delay_slot = mem::take(&mut self.cpu.pending_jump);
            if execution.last_in_delay_slot && delay_slot.happen {
                self.cpu.pc = delay_slot.target;
            } else {
                self.cpu.pc = execution.last_pc.wrapping_add(4);
            }
        }
    }
}
