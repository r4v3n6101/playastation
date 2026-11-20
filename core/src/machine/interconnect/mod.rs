use core::ops::Range;

pub mod bus;
pub(crate) mod scheduler;

/// 2MiB of mapped RAM.
pub const RAM_SIZE: usize = 2 * 1024 * 1024;

/// 512KiB BIOS, ROM.
pub const BIOS_SIZE: usize = 512 * 1024;

/// RAM takes 8MiB, but 3 others are mirrors to the first 2MiB
const RAM: Range<u32> = 0x0000_0000..0x0080_0000;
const EXPANSION1: Range<u32> = 0x1F00_0000..0x1F80_0000;
const SCRATCHPAD: Range<u32> = 0x1F80_0000..0x1F80_0400;
const HW_REGS: Range<u32> = 0x1F80_1000..0x1F80_2000;
const JOY_BUS: Range<u32> = 0x1F80_1040..0x1F80_1050;
const INT_CTRL: Range<u32> = 0x1F80_1070..0x1F80_1078;
const DMA_CTRL: Range<u32> = 0x1F80_1080..0x1F80_1100;
const TIMER_CTRL: Range<u32> = 0x1F80_1100..0x1F80_1130;
const CDROM: Range<u32> = 0x1F80_1800..0x1F80_1804;
const GPU: Range<u32> = 0x1F80_1810..0x1F80_1818;
const SPU: Range<u32> = 0x1F80_1C00..0x1F80_2000;
const EXPANSION2: Range<u32> = 0x1F80_2000..0x1F80_3000;
const BIOS: Range<u32> = 0x1FC0_0000..0x1FC8_0000;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Region {
    Ram,
    Bios,
    Scratchpad,
    Expansion1,
    Expansion2,
    Joy,
    Int,
    Dma,
    Timer,
    CdRom,
    Gpu,
    Spu,
    HwRegs,
    Unmapped,
}

pub fn region_of(paddr: u32) -> Region {
    match paddr {
        x if RAM.contains(&x) => Region::Ram,
        x if BIOS.contains(&x) => Region::Bios,
        x if SCRATCHPAD.contains(&x) => Region::Scratchpad,
        x if EXPANSION1.contains(&x) => Region::Expansion1,
        x if EXPANSION2.contains(&x) => Region::Expansion2,
        x if JOY_BUS.contains(&x) => Region::Joy,
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
