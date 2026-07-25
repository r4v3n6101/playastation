use crate::{
    cpu::{Cpu, Exception},
    interconnect::Bus,
    scheduler::Cycle,
};

mod cache;
mod decoder;
mod interpreter;

const BLOCK_THRESHOLD: Cycle = 8;

pub enum StopReason {
    UnitEnded,
    BudgetExhausted,
    Stalled,
    ExeLoad,
    Print(char),
    Exception(Exception),
}

pub struct ExecutionResult {
    pub cycles_elapsed: Cycle,
    pub stop_reason: StopReason,
}

#[derive(Default)]
pub struct CpuEngine {
    cache: cache::CodeCache,
}

impl CpuEngine {
    pub fn cache_invalidate_all(&mut self) {
        self.cache.invalidate_all();
    }

    pub fn cache_invalidate_by_addr(&mut self, paddr: u32) {
        self.cache.invalidate_addr(paddr);
    }

    pub fn run_for(&mut self, cpu: &mut Cpu, bus: &mut Bus, budget: Cycle) -> ExecutionResult {
        let mut result = ExecutionResult {
            cycles_elapsed: 0,
            stop_reason: StopReason::BudgetExhausted,
        };

        while result.cycles_elapsed < budget {
            if cpu.refresh_interrupt_pending(bus) {
                result.stop_reason = StopReason::Exception(Exception::Interrupt);
                break;
            }

            let before = result.cycles_elapsed;
            let remaining = budget - before;

            result.stop_reason = StopReason::UnitEnded;
            self.run_unit(&mut result, cpu, bus, remaining);

            // Prevent deadloop
            if result.cycles_elapsed == before {
                result.stop_reason = StopReason::Stalled;
            }

            match result.stop_reason {
                StopReason::UnitEnded => {}
                _ => break,
            }
        }

        result
    }

    fn run_unit(
        &mut self,
        result: &mut ExecutionResult,
        cpu: &mut Cpu,
        bus: &mut Bus,
        budget: Cycle,
    ) {
        if budget < BLOCK_THRESHOLD {
            interpreter::run_single(result, cpu, bus, &mut self.cache);
        } else {
            interpreter::run_block(result, cpu, bus, &mut self.cache);
        }

        self.catch_bios_hook(result, cpu);
    }

    fn catch_bios_hook(&self, result: &mut ExecutionResult, cpu: &Cpu) {
        if cpu.pc == 0x80030000 {
            result.stop_reason = StopReason::ExeLoad;
        } else if (cpu.pc == 0xA0 && cpu.gpr[9] == 0x3C) || (cpu.pc == 0xB0 && cpu.gpr[9] == 0x3D) {
            result.stop_reason = StopReason::Print(cpu.gpr[4] as u8 as char);
        }
    }
}
