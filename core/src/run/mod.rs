use alloc::{string::String, vec::Vec};

use crate::{
    cpu::{Cpu, Exception, PendingLoad},
    interconnect::Bus,
};

mod block;
mod decoder;
mod interpreter;

// TODO : rename
#[derive(Debug)]
pub struct CpuExecutor {
    pub cpu: Cpu,
    /// Maximum block size. If the last op is branch delay, block may be max+1
    pub block_size: usize,

    /// Cache of decoded block of ops
    block: Vec<decoder::Operation>,
    /// TTY line.
    tty_line: String,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct ExecutionResult {
    pub last_pc: u32,
    pub last_in_delay_slot: bool,
    pub jump: bool,
    pub jump_target: u32,
    pub cycles_elapsed: u64,
    pub exception: Option<Exception>,
}

impl Default for CpuExecutor {
    fn default() -> Self {
        // TODO : block may be invalidated by stores from the inside
        const DEFAULT_INS_BLOCK: usize = 1;

        Self {
            cpu: Cpu::default(),

            block_size: DEFAULT_INS_BLOCK,
            block: Vec::with_capacity(DEFAULT_INS_BLOCK + 1),
            tty_line: String::new(),
        }
    }
}

impl CpuExecutor {
    pub fn run(&mut self, bus: &mut Bus) {
        // Decode batch of instructions, stopping at an error in fetch/decode or Syscall/Break.
        decoder::fetch_and_decode_block(&mut self.block, self.block_size, &mut self.cpu, bus);

        // CPU first
        let execution = interpreter::run(&self.block, &mut self.cpu, bus);
        let next_pc = if execution.jump {
            execution.jump_target
        } else {
            execution.last_pc.wrapping_add(4)
        };

        // Then devices on the bus are updated
        bus.update(execution.cycles_elapsed);

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

        // Interrupt changes flow like it's an error occurred in the last op
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
