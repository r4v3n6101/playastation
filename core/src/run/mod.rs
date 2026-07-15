use alloc::{boxed::Box, string::String};
use core::mem;

use crate::{
    cpu::{Cpu, Exception, PendingJump},
    formats::psexe::{BoxedExeFile, ExeHeader},
    interconnect::Bus,
    scheduler::Cycle,
};

mod backend;

const MAX_BUDGET: Cycle = 128;

#[derive(Default)]
pub struct Console {
    pub cpu: Cpu,
    pub bus: Bus,
    pub pending_exe: Option<BoxedExeFile>,
    pub printf: Option<Box<dyn FnMut(char)>>,
    /// CPU engine for code execution, with some optimizations.
    engine: backend::CpuEngine,
}

impl Console {
    pub fn step(&mut self) -> u64 {
        // May be scheduler in the future
        let budget = MAX_BUDGET;

        // TODO : budget not working, large values screw games up
        let result = self.engine.run_for(&mut self.cpu, &mut self.bus, budget);

        // TODO : more precise timings
        // TODO : remove
        let sys_cycles = self.bus.update(result.cycles_elapsed, |paddr| {
            self.engine.cache_invalidate_by_addr(paddr);
        });

        match result.stop_reason {
            backend::StopReason::Print(ch) => self.print_char(ch),
            backend::StopReason::ExeLoad => self.load_exe(),
            backend::StopReason::Exception(exc) => self.handle_exception(exc),
            _ => {}
        }

        sys_cycles
    }

    fn print_char(&mut self, ch: char) {
        if let Some(printf) = &mut self.printf {
            (printf)(ch)
        }
    }

    fn load_exe(&mut self) {
        if let Some(exe) = self.pending_exe.take() {
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
            self.engine.cache_invalidate_all();

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

    fn handle_exception(&mut self, exception: Exception) {
        // Commit pending load in a slow, but safe way
        // Safe guard if one writes a garbage
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
    }
}
