use alloc::vec::Vec;
use core::ops::Range;

use crate::{
    devices::{
        Mmio, dma::DmaController, gpu::Gpu, int::InterruptController, timer::TimerController,
    },
    globals::{BIOS_SIZE, RAM_SIZE},
};

/// RAM takes 8MiB, but 3 others are mirrors to the first 2MiB
const RAM: Range<u32> = 0x0000_0000..0x007F_FFFF;
const EXPANSION1: Range<u32> = 0x1F00_0000..0x1F7F_FFFF;
const SCRATCHPAD: Range<u32> = 0x1F80_0000..0x1F80_03FF;

const HW_REGS: Range<u32> = 0x1F80_1000..0x1F80_1FFF;

const INT_CTRL: Range<u32> = 0x1F80_1070..0x1F80_1078;
const DMA_CTRL: Range<u32> = 0x1F80_1080..0x1F80_10FF;
const TIMER_CTRL: Range<u32> = 0x1F80_1100..0x1F80_1130;
const CDROM: Range<u32> = 0x1F80_1800..0x1F80_1803;
const GPU: Range<u32> = 0x1F80_1810..0x1F80_1818;
const SPU: Range<u32> = 0x1F80_1C00..0x1F80_1FFF;

const EXPANSION2: Range<u32> = 0x1F80_2000..0x1F80_2FFF;
const BIOS: Range<u32> = 0x1FC0_0000..0x1FC7_FFFF;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Region {
    Ram,
    Bios,
    Scratchpad,
    Expansion1,
    Expansion2,
    Int,
    Dma,
    Timer,
    CdRom,
    Gpu,
    Spu,
    HwRegs,
    Unmapped,
}

pub struct Bus {
    pub bios: Vec<u8>,
    pub ram: Vec<u8>,

    // Devices
    pub int_ctrl: InterruptController,
    pub dma_ctrl: DmaController,
    pub timer_ctrl: TimerController,
    pub gpu: Gpu,
}

impl Default for Bus {
    fn default() -> Self {
        let bios = alloc::vec![0; BIOS_SIZE];
        let ram = alloc::vec![0; RAM_SIZE];

        Self {
            bios,
            ram,

            int_ctrl: InterruptController::default(),
            dma_ctrl: DmaController::default(),
            timer_ctrl: TimerController::default(),
            gpu: Gpu::default(),
        }
    }
}

impl Bus {
    /// Return PSX RAM as host RAM
    pub fn direct_ram(&mut self) -> &mut [u8] {
        &mut self.ram
    }

    pub fn load<const N: usize>(&mut self, paddr: u32) -> [u8; N] {
        let mut bytes = [0; N];

        let mmio_span = tracing::trace_span!(
            target: "bus.mmio",
            "load",
            addr=%format_args!("{paddr:#X}")
        );
        match region_of(paddr) {
            Region::Ram => {
                let addr = ((paddr - RAM.start) as usize) % RAM_SIZE;
                bytes.copy_from_slice(&self.ram[addr..][..N]);
            }
            Region::Bios => {
                bytes.copy_from_slice(&self.bios[(paddr - BIOS.start) as usize..][..N]);
            }
            Region::Scratchpad => {}
            Region::Expansion1 => {}
            Region::Expansion2 => {}
            Region::Int => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - INT_CTRL.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "int ctrl read");
                self.int_ctrl.read(&mut bytes, mmio_addr);
            }
            Region::Dma => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - DMA_CTRL.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "dma ctrl read");
                self.dma_ctrl.read(&mut bytes, mmio_addr);
            }
            Region::Timer => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - TIMER_CTRL.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "timer ctrl read");
                self.timer_ctrl.read(&mut bytes, mmio_addr);
            }
            Region::CdRom => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - CDROM.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "cdrom read");
            }
            Region::Gpu => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - GPU.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "gpu read");
                self.gpu.read(&mut bytes, mmio_addr);
            }
            Region::Spu => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - SPU.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "spu read");
            }
            Region::HwRegs => {
                let _guard = mmio_span.enter();
                tracing::trace!(translated_addr=%format_args!("{paddr:#X}"), "HW regs touched");
            }
            Region::Unmapped => {}
        }

        bytes
    }

    pub fn store<const N: usize>(&mut self, paddr: u32, value: [u8; N]) {
        let mmio_span = tracing::trace_span!(
            target: "bus.mmio",
            "store",
            addr=%format_args!("{paddr:#X}"),
            ?value
        );
        match region_of(paddr) {
            Region::Ram => {
                self.ram[(paddr - RAM.start) as usize..][..N].copy_from_slice(&value);
            }
            Region::Bios => {
                self.bios[(paddr - BIOS.start) as usize..][..N].copy_from_slice(&value);
            }
            Region::Scratchpad => {}
            Region::Expansion1 => {}
            Region::Expansion2 => {}
            Region::Int => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - INT_CTRL.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "int ctrl write");
                self.int_ctrl.write(mmio_addr, &value);
            }
            Region::Dma => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - DMA_CTRL.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "dma ctrl write");
                self.dma_ctrl.write(mmio_addr, &value);
            }
            Region::Timer => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - TIMER_CTRL.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "timer ctrl write");
                self.timer_ctrl.write(mmio_addr, &value);
            }
            Region::CdRom => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - CDROM.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "cdrom write");
            }
            Region::Gpu => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - GPU.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "gpu write");
                self.gpu.write(mmio_addr, &value);
            }
            Region::Spu => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - SPU.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "spu write");
            }
            Region::HwRegs => {
                let _guard = mmio_span.enter();
                tracing::trace!(translated_addr=%format_args!("{paddr:#X}"), "HW regs touched");
            }
            Region::Unmapped => {}
        }
    }
}

pub fn region_of(paddr: u32) -> Region {
    match paddr {
        x if RAM.contains(&x) => Region::Ram,
        x if BIOS.contains(&x) => Region::Bios,
        x if SCRATCHPAD.contains(&x) => Region::Scratchpad,
        x if EXPANSION1.contains(&x) => Region::Expansion1,
        x if EXPANSION2.contains(&x) => Region::Expansion2,
        x if INT_CTRL.contains(&x) => Region::Int,
        x if DMA_CTRL.contains(&x) => Region::Dma,
        x if TIMER_CTRL.contains(&x) => Region::Timer,
        x if CDROM.contains(&x) => Region::CdRom,
        x if GPU.contains(&x) => Region::Gpu,
        x if SPU.contains(&x) => Region::Spu,
        x if HW_REGS.contains(&x) => Region::HwRegs,
        _ => Region::Unmapped,
    }
}
