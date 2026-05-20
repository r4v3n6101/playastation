use alloc::string::String;

use crate::{
    cpu::{Cpu, Exception, PendingLoad},
    devices::{dma::DmaController, gpu::Gpu, timer::TimerController},
    interconnect::Bus,
};

mod block;
mod interpreter;

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
struct ExecutionResult {
    last_pc: u32,
    last_in_delay_slot: bool,
    jump: bool,
    jump_target: u32,
    cycles_elapsed: u64,
    exception: Option<Exception>,
}

#[derive(Debug, Default)]
pub struct Executor {
    pub cpu: Cpu,
    /// Cache for fetched & decoded blocks
    blk_cache: block::PagedCache,
    /// TTY line.
    tty_line: String,
}

impl Executor {
    pub fn run(&mut self, bus: &mut Bus) {
        // Cache fetch & decode of blocks
        let block = self.blk_cache.get_or_fetch_decode_block(&mut self.cpu, bus);

        // CPU first
        let execution = interpreter::run(&mut self.blk_cache, block, &mut self.cpu, bus);
        let next_pc = if execution.jump {
            execution.jump_target
        } else {
            execution.last_pc.wrapping_add(4)
        };

        // Then devices on the bus are updated
        self.update_devices(execution.cycles_elapsed, bus);

        self.cpu.cop0.set_hw_irq(bus.int_ctrl.pending());
        let interrupt = self
            .cpu
            .cop0
            .interrupt_pending()
            .then_some(ExecutionResult {
                exception: Some(Exception::Interrupt),
                // EPC must be set to the next instruction, so it either pc+4 or jump target
                last_pc: next_pc,
                // The last called instruction may be in delay slot, but the next must not be
                // I.e. the last op must not be branch
                last_in_delay_slot: false,
                ..Default::default()
            });

        // Interrupt changes flow like it's an error occurred in the last op,
        // but EPC set to the next ins
        let execution = interrupt.unwrap_or(execution);
        if let Some(exception) = execution.exception {
            tracing::debug!(
                ?exception,
                epc=%format_args!("{:#X}", execution.last_pc),
                delay_slot=%execution.last_in_delay_slot,
                "entering exception handler"
            );

            self.cpu.write_delayed(PendingLoad::default());

            self.cpu.cop0.exception_enter(
                exception,
                execution.last_pc,
                execution.last_in_delay_slot,
            );
            self.cpu.pc = self.cpu.cop0.exception_handler();
        } else {
            self.cpu.pc = next_pc;
            self.handle_tty();
        }
    }

    fn update_devices(&mut self, cpu_cycles: u64, bus: &mut Bus) {
        let dma_cycles = DmaController::run(bus, |paddr| {
            self.blk_cache.invalidate_page(paddr);
        });

        Gpu::run(bus);

        let sys_cycles = cpu_cycles.saturating_add(dma_cycles);
        TimerController::update(bus, sys_cycles);
    }

    fn handle_tty(&mut self) {
        if (self.cpu.pc == 0xA0 && self.cpu.gpr[9] == 0x3C)
            || (self.cpu.pc == 0xB0 && self.cpu.gpr[9] == 0x3D)
        {
            match self.cpu.gpr[4] as u8 as char {
                '\n' => {
                    tracing::info!(target: "tty", "{}", self.tty_line);
                    self.tty_line.clear();
                }
                '\r' => {}
                ch => {
                    self.tty_line.push(ch);
                }
            }
        }
    }
}
