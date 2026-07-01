use alloc::string::String;
use core::mem;

use crate::{
    cpu::{Cpu, Exception, PendingJump},
    formats::psexe::{BoxedExeFile, ExeHeader},
    interconnect::Bus,
};

mod block;
mod interpreter;

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
struct ExecutionResult {
    cycles_elapsed: u64,
    exception: Option<Exception>,
}

#[derive(Default)]
pub struct Executor {
    pub cpu: Cpu,
    pub bus: Bus,
    pub pending_exe: Option<BoxedExeFile>,
    /// Cache for fetched & decoded blocks
    blk_cache: block::PagedCache,
    /// TTY line.
    tty_line: String,
}

impl Executor {
    pub fn run(&mut self) -> u64 {
        // Cache fetch & decode of blocks
        let block = self
            .blk_cache
            .get_or_fetch_decode_block(&mut self.cpu, &mut self.bus);

        // CPU first
        let execution = interpreter::run(&mut self.blk_cache, &block, &mut self.cpu, &mut self.bus);

        // Then devices on the bus are updated
        // TODO : more precise timings
        let sys_cycles = self.bus.update(execution.cycles_elapsed, |paddr| {
            self.blk_cache.invalidate_page(paddr);
        });

        self.cpu
            .cop0
            .set_hw_irq(self.cpu.pending_jump.is_none() && self.bus.int_ctrl.pending());
        let interrupt = self
            .cpu
            .cop0
            .interrupt_pending()
            .then_some(ExecutionResult {
                exception: Some(Exception::Interrupt),
                ..Default::default()
            });

        // Interrupt changes flow like it's an error occurred before the next op/or after delay slot.
        let execution = interrupt.unwrap_or(execution);
        if let Some(exception) = execution.exception {
            // Commit pending load in a slow, but safe way
            // In case one of instruction executor will write a garbage
            let pending_load = mem::take(&mut self.cpu.pending_load);
            self.cpu.gpr[pending_load.dest as usize] = pending_load.value;
            self.cpu.gpr[0] = 0;

            tracing::debug!(?exception, cpu=?self.cpu, "entering exception handler");
            self.cpu.cop0.exception_enter(
                exception,
                self.cpu.pc,
                self.cpu.pending_jump.map(PendingJump::target),
            );
            self.cpu.pc = self.cpu.cop0.exception_handler();
        } else {
            self.handle_tty();
            self.handle_exe();
        }

        sys_cycles
    }

    fn handle_exe(&mut self) {
        if self.cpu.pc == 0x80030000
            && let Some(exe) = self.pending_exe.take()
        {
            let hdr @ ExeHeader {
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
                    .write_bus(
                        &mut self.bus,
                        ram_dest.get().wrapping_add(i),
                        [prog[i as usize]],
                    )
                    .expect("valid ram dest in exe file");
            }
            self.blk_cache.clear();

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
            tracing::info!(?hdr, %text, "PS-EXE loaded");
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
