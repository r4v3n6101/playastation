use std::ops::Range;

use crate::devices::{
    Mmio, dma::DmaController, gpu::Gpu, int::InterruptController, timer::TimerController,
};

// MIPS uses segmented memory, but PSX ignore them and treat all segments as mirror to each other
const KUSEG: Range<u32> = 0x0000_0000..0x7FFF_FFFF;
const KSEG0: Range<u32> = 0x8000_0000..0x9FFF_FFFF;
const KSEG1: Range<u32> = 0xA000_0000..0xBFFF_FFFF;
const CACHE_CONTROL: u32 = 0xFFFE_0130;

// Mapped memory
const RAM: Range<u32> = 0x0000_0000..0x001F_FFFF;
const EXPANSION1: Range<u32> = 0x1F00_0000..0x1F7F_FFFF;
const SCRATCHPAD: Range<u32> = 0x1F80_0000..0x1F80_03FF;

// Hardware registers regions
const HW_REGS: Range<u32> = 0x1F801000..0x1F801FFF;
const INT_CTRL: Range<u32> = 0x1F80_1070..0x1F80_1078;
const DMA_CTRL: Range<u32> = 0x1F80_1080..0x1F80_10FF;
const TIMER_CTRL: Range<u32> = 0x1F80_1100..0x1F80_1130;
const GPU: Range<u32> = 0x1F80_1810..0x1F80_1818;

const MISC: Range<u32> = 0x1F80_2000..0x1F80_2FFF;
const EXPANSION2: Range<u32> = 0x1FA0_0000..0x1FA1_FFFF;
const BIOS: Range<u32> = 0x1FC0_0000..0x1FC7_FFFF;

#[derive(Debug)]
pub struct BusError {
    pub bad_vaddr: u32,
    pub kind: BusErrorKind,
}

#[derive(Debug)]
pub enum BusErrorKind {
    UnalignedAddr,
    Unmapped,
}

pub struct Bus {
    pub bios: Vec<u8>,
    pub ram: Vec<u8>,
    pub misc: Vec<u8>,
    pub scratchpad: Vec<u8>,
    pub expansion1: Vec<u8>,
    pub expansion2: Vec<u8>,

    // Devices
    pub int_ctrl: InterruptController,
    pub dma_ctrl: DmaController,
    pub timer_ctrl: TimerController,
    pub gpu: Gpu,
}

impl Default for Bus {
    fn default() -> Self {
        let bios = vec![0; BIOS.len() + 1];
        let ram = vec![0; RAM.len() + 1];
        let misc = vec![0; MISC.len() + 1];
        let scratchpad = vec![0; SCRATCHPAD.len() + 1];
        let expansion1 = vec![0; EXPANSION1.len() + 1];
        let expansion2 = vec![0; EXPANSION2.len() + 1];

        Self {
            bios,
            ram,
            misc,
            scratchpad,
            expansion1,
            expansion2,

            int_ctrl: InterruptController::default(),
            dma_ctrl: DmaController::default(),
            timer_ctrl: TimerController::default(),
            gpu: Gpu::default(),
        }
    }
}

impl Bus {
    pub fn update(&mut self, cpu_cycles: u64) {
        let dma_cycles = DmaController::run(self);
    }

    pub fn load<const N: usize>(&mut self, addr: u32) -> Result<[u8; N], BusError> {
        if !addr.is_multiple_of(N as u32) {
            return Err(BusError {
                bad_vaddr: addr,
                kind: BusErrorKind::UnalignedAddr,
            });
        }

        let mut bytes = [0; N];

        let mmio_span = tracing::trace_span!(
            target: "bus.mmio",
            "load",
            addr=%format_args!("{addr:#X}")
        );
        match translate_addr(addr)? {
            x if RAM.contains(&x) => {
                bytes.copy_from_slice(&self.ram[(x - RAM.start) as usize..][..N]);
            }
            x if EXPANSION1.contains(&x) => {
                bytes.copy_from_slice(&self.expansion1[(x - EXPANSION1.start) as usize..][..N]);
            }
            x if SCRATCHPAD.contains(&x) => {
                bytes.copy_from_slice(&self.scratchpad[(x - SCRATCHPAD.start) as usize..][..N]);
            }
            x if MISC.contains(&x) => {
                bytes.copy_from_slice(&self.misc[(x - MISC.start) as usize..][..N]);
            }
            x if EXPANSION2.contains(&x) => {
                bytes.copy_from_slice(&self.expansion2[(x - EXPANSION2.start) as usize..][..N]);
            }
            x if BIOS.contains(&x) => {
                bytes.copy_from_slice(&self.bios[(x - BIOS.start) as usize..][..N]);
            }
            x if INT_CTRL.contains(&x) => {
                let mmio_addr = x - INT_CTRL.start;
                mmio_span.in_scope(|| {
                    tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "int ctrl read");
                    self.int_ctrl.read(&mut bytes, mmio_addr);
                });
            }
            x if DMA_CTRL.contains(&x) => {
                let mmio_addr = x - DMA_CTRL.start;
                mmio_span.in_scope(|| {
                    tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "dma ctrl read");
                    self.dma_ctrl.read(&mut bytes, mmio_addr);
                });
            }
            x if TIMER_CTRL.contains(&x) => {
                let mmio_addr = x - TIMER_CTRL.start;
                mmio_span.in_scope(|| {
                    tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "timer ctrl read");
                    self.timer_ctrl.read(&mut bytes, mmio_addr);
                });
            }
            x if GPU.contains(&x) => {
                let mmio_addr = x - GPU.start;
                mmio_span.in_scope(|| {
                    tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "gpu read");
                    self.gpu.read(&mut bytes, mmio_addr);
                });
            }
            x if HW_REGS.contains(&x) => {
                let _guard = mmio_span.enter();
                tracing::trace!(translated_addr=%format_args!("{x:#X}"), "HW regs touched");
            }
            _ => {
                return Err(BusError {
                    bad_vaddr: addr,
                    kind: BusErrorKind::Unmapped,
                });
            }
        }

        Ok(bytes)
    }

    pub fn store<const N: usize>(&mut self, addr: u32, value: [u8; N]) -> Result<(), BusError> {
        if !addr.is_multiple_of(N as u32) {
            return Err(BusError {
                bad_vaddr: addr,
                kind: BusErrorKind::UnalignedAddr,
            });
        }

        let mmio_span = tracing::trace_span!(
            target: "bus.mmio",
            "store",
            addr=%format_args!("{addr:#X}"),
            ?value
        );
        match translate_addr(addr)? {
            x if RAM.contains(&x) => {
                self.ram[(x - RAM.start) as usize..][..N].copy_from_slice(&value);
            }
            x if EXPANSION1.contains(&x) => {
                self.expansion1[(x - EXPANSION1.start) as usize..][..N].copy_from_slice(&value);
            }
            x if SCRATCHPAD.contains(&x) => {
                self.scratchpad[(x - SCRATCHPAD.start) as usize..][..N].copy_from_slice(&value);
            }
            x if MISC.contains(&x) => {
                self.misc[(x - MISC.start) as usize..][..N].copy_from_slice(&value);
            }
            x if EXPANSION2.contains(&x) => {
                self.expansion2[(x - EXPANSION2.start) as usize..][..N].copy_from_slice(&value);
            }
            x if BIOS.contains(&x) => {
                self.bios[(x - BIOS.start) as usize..][..N].copy_from_slice(&value);
            }
            x if INT_CTRL.contains(&x) => {
                let mmio_addr = x - INT_CTRL.start;
                mmio_span.in_scope(|| {
                    tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "int ctrl write");
                    self.int_ctrl.write(mmio_addr, &value);
                });
            }
            x if DMA_CTRL.contains(&x) => {
                let mmio_addr = x - DMA_CTRL.start;
                mmio_span.in_scope(|| {
                    tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "dma ctrl write");
                    self.dma_ctrl.write(mmio_addr, &value);
                });
            }
            x if TIMER_CTRL.contains(&x) => {
                let mmio_addr = x - TIMER_CTRL.start;
                mmio_span.in_scope(|| {
                    tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "timer ctrl write");
                    self.timer_ctrl.write(mmio_addr, &value);
                });
            }
            x if GPU.contains(&x) => {
                let mmio_addr = x - GPU.start;
                mmio_span.in_scope(|| {
                    tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "gpu write");
                    self.gpu.write(mmio_addr, &value);
                });
            }
            x if HW_REGS.contains(&x) => {
                let _guard = mmio_span.enter();
                tracing::trace!(translated_addr=%format_args!("{x:#X}"), "HW regs touched");
            }
            _ => {
                return Err(BusError {
                    bad_vaddr: addr,
                    kind: BusErrorKind::Unmapped,
                });
            }
        }

        Ok(())
    }
}

// Translate virtual address from segments into physical one.
fn translate_addr(addr: u32) -> Result<u32, BusError> {
    match addr {
        x if KUSEG.contains(&x) || KSEG0.contains(&x) || KSEG1.contains(&x) => {
            Ok(addr & 0x1FFF_FFFF)
        }
        CACHE_CONTROL => {
            // TODO
            Ok(0)
        }
        _ => Err(BusError {
            bad_vaddr: addr,
            kind: BusErrorKind::Unmapped,
        }),
    }
}
