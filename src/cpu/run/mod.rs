use std::mem;

use crate::{
    cpu::{Cpu, Exception},
    interconnect::Bus,
};

pub mod interpreter;

pub trait Executor {
    fn run(&mut self, cpu: &mut Cpu, bus: &mut Bus) -> Result<(), Exception>;
}

#[derive(Debug, Default)]
pub struct CpuExecutor<E> {
    pub executor: E,
    pub cpu: Cpu,
}

impl<E> CpuExecutor<E>
where
    E: Executor,
{
    pub fn cycle(&mut self, bus: &mut Bus) {
        self.cpu.cop0.set_hw_irq(bus.int_ctrl.pending());

        let interrupt = self
            .cpu
            .cop0
            .interrupt_pending()
            .then_some(Exception::Interrupt);

        let load_delay_slot = mem::take(&mut self.cpu.pending_load);
        let branch_delay_slot = mem::take(&mut self.cpu.pending_jump);
        let exception = self.executor.run(&mut self.cpu, bus).err();
        if let Some(exception) = interrupt.or(exception) {
            self.cpu
                .cop0
                .exception_enter(exception, self.cpu.pc, branch_delay_slot.has_delay_slot);
            self.cpu.pc = self.cpu.cop0.exception_handler();
        } else {
            if branch_delay_slot.happen {
                self.cpu.pc = branch_delay_slot.target;
            } else {
                self.cpu.pc = self.cpu.pc.wrapping_add(4);
            }
        }

        if load_delay_slot.dest != 0 {
            self.cpu.gpr[load_delay_slot.dest] = load_delay_slot.value;
        }
    }
}
