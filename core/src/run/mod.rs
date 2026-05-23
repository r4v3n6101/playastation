use alloc::string::String;
use core::mem;

use crate::{
    cpu::{Cpu, Exception, PendingJump},
    formats::{BoxedExeFile, ExeHeader},
    interconnect::Bus,
};

mod block;
mod interpreter;

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
struct ExecutionResult {
    last_pc: u32,
    next_pc: u32,
    last_in_delay_slot: bool,
    cycles_elapsed: u64,
    exception: Option<Exception>,
}

#[derive(Debug, Default)]
pub struct Executor {
    pub cpu: Cpu,
    pub pending_exe: Option<BoxedExeFile>,
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
        let execution = interpreter::run(&mut self.blk_cache, &block, &mut self.cpu, bus);

        // Then devices on the bus are updated
        bus.update(execution.cycles_elapsed, |paddr| {
            self.blk_cache.invalidate_page(paddr);
        });

        let can_take_interrupt = !self.cpu.pending_jump.valid;
        self.cpu
            .cop0
            .set_hw_irq(can_take_interrupt && bus.int_ctrl.pending());
        let interrupt = self
            .cpu
            .cop0
            .interrupt_pending()
            .then_some(ExecutionResult {
                exception: Some(Exception::Interrupt),
                last_pc: execution.next_pc,
                last_in_delay_slot: false,
                ..Default::default()
            });

        // Interrupt changes flow like it's an error occurred before the next op/or after delay slot.
        let execution = interrupt.unwrap_or(execution);
        if let Some(exception) = execution.exception {
            tracing::debug!(
                ?exception,
                epc=%format_args!("{:#X}", execution.last_pc),
                delay_slot=%execution.last_in_delay_slot,
                "entering exception handler"
            );

            // Reset jump
            self.cpu.pending_jump = PendingJump::default();
            // Commit pending load in a slow, but safe way
            // In case one of instruction executor will write a garbage
            let pending_load = mem::take(&mut self.cpu.pending_load);
            self.cpu.gpr[pending_load.dest as usize] = pending_load.value;
            self.cpu.gpr[0] = 0;

            self.cpu.cop0.exception_enter(
                exception,
                execution.last_pc,
                execution.last_in_delay_slot,
            );
            self.cpu.pc = self.cpu.cop0.exception_handler();
        } else {
            self.cpu.pc = execution.next_pc;

            self.handle_tty();
            self.handle_exe(bus);
        }
    }

    fn handle_exe(&mut self, bus: &mut Bus) {
        if self.cpu.pc == 0x80030000
            && let Some(exe) = self.pending_exe.take()
        {
            let ExeHeader {
                ipc,
                igp,
                file_size,
                ispb,
                ispoff,
                ram_dest,
                text,
                ..
            } = exe.header;
            let prog = &exe.prog;

            for i in 0..file_size.get() {
                self.cpu
                    .write_bus(bus, ram_dest.get().wrapping_add(i), [prog[i as usize]])
                    .expect("valid ram dest in exe file");
            }

            self.cpu.pc = ipc.get();
            self.cpu.gpr[28] = igp.get();
            if ispb.get() != 0 {
                self.cpu.gpr[29] = ispb.get() + ispoff.get();
                self.cpu.gpr[30] = self.cpu.gpr[29];
            }

            let text = text
                .iter()
                .copied()
                .take_while(|&c| c != 0)
                .map(|c| c as char)
                .collect::<String>();
            tracing::info!(
                initial_pc=%format_args!("{ipc:#X}"),
                initial_gpr28=%format_args!("{igp:#X}"),
                initial_sp_base=%format_args!("{ispb:#X}"),
                initial_sp_offset=%format_args!("{ispoff:#X}"),
                ram_dest=%format_args!("{ram_dest:#X}"),
                file_size=%format_args!("{file_size:#X}",),
                %text,
                "PS-EXE loaded"
            );
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
