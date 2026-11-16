use alloc::boxed::Box;
use core::cell::Cell;

use crate::{
    SystemTime,
    machine::{
        cpu::{Cpu, Exception, TranslationResult},
        interconnect::bus::Bus,
    },
};

pub use policy::{DefaultPolicy, ExecutionMode, ExecutionPolicy, PolicyContext};

mod cache;
mod decoder;
mod interpreter;
mod policy;

const BLOCK_DECODER_LIMIT: usize = 1024;

pub struct ExecutionResult {
    pub cycles_elapsed: SystemTime,
    pub stop_reason: StopReason,
}

#[derive(Debug)]
pub enum StopReason {
    UnitEnded,
    Stalled,
    ExeLoad,
    Print(char),
    Exception(Exception),
}

pub struct CpuEngine {
    pub policy: Box<dyn ExecutionPolicy>,
    cache: cache::CodeCache,
}

impl Default for CpuEngine {
    fn default() -> Self {
        Self {
            policy: Box::new(DefaultPolicy),
            cache: cache::CodeCache::default(),
        }
    }
}

impl CpuEngine {
    pub fn cache_invalidate_all(&mut self) {
        self.cache.invalidate_all();
    }

    pub fn cache_invalidate_by_addr(&mut self, paddr: u32) {
        self.cache.invalidate_addr(paddr);
    }

    pub fn run_for(&mut self, cpu: &mut Cpu, bus: &mut Bus, budget: SystemTime) -> ExecutionResult {
        let mut result = ExecutionResult {
            cycles_elapsed: 0,
            stop_reason: StopReason::UnitEnded,
        };

        while result.cycles_elapsed < budget {
            if cpu.refresh_interrupt_pending(bus) {
                result.stop_reason = StopReason::Exception(Exception::Interrupt);
                break;
            }

            let before = result.cycles_elapsed;
            let remaining = budget - result.cycles_elapsed;
            self.run_unit(&mut result, cpu, bus, remaining);

            if let StopReason::UnitEnded = result.stop_reason {
                // Prevent deadloop
                if result.cycles_elapsed == before {
                    result.stop_reason = StopReason::Stalled;
                } else {
                    continue;
                }
            }

            break;
        }

        result
    }

    fn run_unit(
        &mut self,
        result: &mut ExecutionResult,
        cpu: &mut Cpu,
        bus: &mut Bus,
        budget: SystemTime,
    ) {
        // TODO : don't like searching a block for a dead simple ins-by-ins interpreter
        let block = self.cache.get(cpu);
        let mode = self.policy.select(PolicyContext {
            budget,
            hits: block.as_ref().map_or(0, |block| block.hits.get()),
        });

        match mode {
            ExecutionMode::Interpreter => {
                interpreter::run_single(result, cpu, bus, &mut self.cache);
            }
            ExecutionMode::CachedInterpreter => {
                let block = block.unwrap_or_else(|| {
                    let TranslationResult::PhysAddr(paddr) = cpu.mmu.translate_addr(cpu.pc) else {
                        panic!("non-executable memory");
                    };

                    self.cache.insert(
                        paddr,
                        cache::CodeBlock {
                            ops: decoder::fetch_and_decode_block(cpu, bus, BLOCK_DECODER_LIMIT),
                            hits: Cell::new(0),
                        },
                    )
                });

                // TODO : there? not sure
                block.hits.update(|hits| hits.saturating_add(1));
                interpreter::run_block(result, cpu, bus, &mut self.cache, &block);
            }
        }

        Self::catch_bios_hook(result, cpu);
    }

    fn catch_bios_hook(result: &mut ExecutionResult, cpu: &Cpu) {
        if cpu.pc == 0x80030000 {
            result.stop_reason = StopReason::ExeLoad;
        } else if (cpu.pc == 0xA0 && cpu.gpr[9] == 0x3C) || (cpu.pc == 0xB0 && cpu.gpr[9] == 0x3D) {
            result.stop_reason = StopReason::Print(cpu.gpr[4] as u8 as char);
        }
    }
}
