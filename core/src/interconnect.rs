use alloc::boxed::Box;
use core::{ops::Range, ptr};

use crate::{
    BIOS_SIZE, RAM_SIZE,
    devices::{
        Mmio, cdrom::CdRom, dma::DmaController, gpu::Gpu, int::InterruptController, joy::JoyBus,
        timer::TimerController,
    },
};

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

pub struct Bus {
    // FIXME: I'd like to size into array, like Box<[T; N]>
    pub bios: Box<[u8]>,
    pub ram: Box<[u8]>,
    pub scratchpad: Box<[u8]>,

    // Devices
    pub int_ctrl: InterruptController,
    pub dma_ctrl: DmaController,
    pub timer_ctrl: TimerController,
    pub cdrom: CdRom,
    pub gpu: Gpu,
    pub joy_bus: JoyBus,
}

impl Default for Bus {
    fn default() -> Self {
        let bios = alloc::vec![0; BIOS_SIZE].into_boxed_slice();
        let ram = alloc::vec![0; RAM_SIZE].into_boxed_slice();
        let scratchpad = alloc::vec![0; SCRATCHPAD.len()].into_boxed_slice();

        Self {
            bios,
            ram,
            scratchpad,

            int_ctrl: InterruptController::default(),
            dma_ctrl: DmaController::default(),
            timer_ctrl: TimerController::default(),
            cdrom: CdRom::default(),
            gpu: Gpu::default(),
            joy_bus: JoyBus::default(),
        }
    }
}

impl Bus {
    #[inline(never)]
    pub fn update(&mut self, cpu_cycles: u64, ram_touched: impl FnMut(u32)) {
        let dma_cycles = DmaController::run(self, ram_touched);
        let sys_cycles = cpu_cycles.saturating_add(dma_cycles);

        self.gpu
            .update(&mut self.int_ctrl, sys_cycles)
            .for_each(|span| {
                self.timer_ctrl.update(&mut self.int_ctrl, span);
            });
        self.joy_bus.update(&mut self.int_ctrl);
        self.cdrom.update(&mut self.int_ctrl);
    }

    // Inlined because RAM/BIOS hot paths are needed for caller
    #[inline(always)]
    pub fn load<const N: usize>(&mut self, paddr: u32) -> [u8; N] {
        let mut buf = [0; N];

        // Aligned access is faster due to check of `start` only
        if paddr.is_multiple_of(N as _) {
            if RAM.contains(&paddr) {
                // SAFETY: Hot path for RAM, `paddr` and `paddr + N` are inside of RAM, so...
                unsafe {
                    let addr = (paddr as usize) & (RAM_SIZE - 1);
                    ptr::copy_nonoverlapping(self.ram.as_ptr().byte_add(addr), buf.as_mut_ptr(), N);
                }

                return buf;
            } else if BIOS.contains(&paddr) {
                // SAFETY: same as above
                unsafe {
                    let addr = (paddr - BIOS.start) as usize;
                    ptr::copy_nonoverlapping(
                        self.bios.as_ptr().byte_add(addr),
                        buf.as_mut_ptr(),
                        N,
                    );
                }

                return buf;
            } else if SCRATCHPAD.contains(&paddr) {
                // SAFETY: same as above
                unsafe {
                    let addr = (paddr - SCRATCHPAD.start) as usize;
                    ptr::copy_nonoverlapping(
                        self.scratchpad.as_ptr().byte_add(addr),
                        buf.as_mut_ptr(),
                        N,
                    );
                }

                return buf;
            }
        }

        self.load_slow_path::<N>(&mut buf, paddr);

        buf
    }

    // Inlining: same as above
    #[inline(always)]
    pub fn store<const N: usize>(&mut self, paddr: u32, value: [u8; N]) {
        // Same as above
        if paddr.is_multiple_of(N as _) {
            if RAM.contains(&paddr) {
                // SAFETY: Hot path for RAM, `paddr` and `paddr + N` are inside of RAM, so...
                unsafe {
                    let addr = (paddr as usize) & (RAM_SIZE - 1);
                    ptr::copy_nonoverlapping(
                        value.as_ptr(),
                        self.ram.as_mut_ptr().byte_add(addr),
                        N,
                    );
                }

                return;
            } else if SCRATCHPAD.contains(&paddr) {
                // SAFETY: as above
                unsafe {
                    let addr = (paddr - SCRATCHPAD.start) as usize;
                    ptr::copy_nonoverlapping(
                        value.as_ptr(),
                        self.scratchpad.as_mut_ptr().byte_add(addr),
                        N,
                    );
                }

                return;
            }
        }

        self.store_slow_path(paddr, value);
    }

    #[cold]
    #[inline(never)]
    fn load_slow_path<const N: usize>(&mut self, buf: &mut [u8], paddr: u32) {
        let mmio_span = tracing::trace_span!(
            target: "bus.mmio",
            "load",
            paddr=%format_args!("{paddr:#X}")
        );
        match region_of(paddr) {
            Region::Expansion1 => {}
            Region::Expansion2 => {}
            Region::Joy => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - JOY_BUS.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "joy bus read");
                self.joy_bus.read(buf, mmio_addr);
            }
            Region::Int => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - INT_CTRL.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "int ctrl read");
                self.int_ctrl.read(buf, mmio_addr);
            }
            Region::Dma => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - DMA_CTRL.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "dma ctrl read");
                self.dma_ctrl.read(buf, mmio_addr);
            }
            Region::Timer => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - TIMER_CTRL.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "timer ctrl read");
                self.timer_ctrl.read(buf, mmio_addr);
            }
            Region::CdRom => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - CDROM.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "cdrom read");
                self.cdrom.read(buf, mmio_addr);
            }
            Region::Gpu => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - GPU.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "gpu read");
                self.gpu.read(buf, mmio_addr);
            }
            Region::Spu => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - SPU.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "spu read");
            }
            Region::HwRegs => {
                let _guard = mmio_span.enter();
                tracing::trace!("HW regs touched");
            }
            // Unaligned access is not implemented and *probably* not used anywhere
            Region::Ram => unimplemented!(),
            Region::Bios => unimplemented!(),
            Region::Scratchpad => unimplemented!(),
            Region::Unmapped => {}
        }
    }

    #[cold]
    #[inline(never)]
    fn store_slow_path<const N: usize>(&mut self, paddr: u32, value: [u8; N]) {
        let mmio_span = tracing::trace_span!(
            target: "bus.mmio",
            "store",
            paddr=%format_args!("{paddr:#X}"),
            ?value
        );
        match region_of(paddr) {
            Region::Expansion1 => {}
            Region::Expansion2 => {}
            Region::Joy => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - JOY_BUS.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "joy bus write");
                self.joy_bus.write(mmio_addr, &value);
            }
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
                self.cdrom.write(mmio_addr, &value);
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
                tracing::trace!("HW regs touched");
            }
            // Unaligned access is not implemented and *probably* not used anywhere
            Region::Ram => unimplemented!(),
            Region::Bios => unimplemented!(),
            Region::Scratchpad => unimplemented!(),
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
