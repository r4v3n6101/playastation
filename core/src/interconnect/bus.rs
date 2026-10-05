use alloc::boxed::Box;
use core::ptr;

use crate::{
    SystemTime,
    devices::{
        cdrom::CdRom, dma::DmaController, gpu::Gpu, int::InterruptController, joy::JoyBus,
        spu::Spu, timer::TimerController,
    },
};

use super::{
    BIOS, BIOS_SIZE, CDROM, DMA_CTRL, GPU, INT_CTRL, JOY_BUS, RAM, RAM_SIZE, Region, SCRATCHPAD,
    SPU, TIMER_CTRL, region_of,
    scheduler::{Event, Scheduler},
};

pub struct Bus {
    pub(crate) scheduler: Scheduler,

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
    pub spu: Spu,
}

impl Default for Bus {
    fn default() -> Self {
        let bios = alloc::vec![0; BIOS_SIZE].into_boxed_slice();
        let ram = alloc::vec![0; RAM_SIZE].into_boxed_slice();
        let scratchpad = alloc::vec![0; SCRATCHPAD.len()].into_boxed_slice();

        let mut scheduler = Scheduler::default();
        for event in [Event::Gpu, Event::Joy, Event::Dma] {
            scheduler.schedule(event, 0);
        }

        Self {
            scheduler,

            bios,
            ram,
            scratchpad,

            int_ctrl: InterruptController::default(),
            dma_ctrl: DmaController::default(),
            timer_ctrl: TimerController::default(),
            cdrom: CdRom::default(),
            gpu: Gpu::default(),
            joy_bus: JoyBus::default(),
            spu: Spu::default(),
        }
    }
}

impl Bus {
    pub(crate) fn handle_event(
        &mut self,
        event: Event,
        ram_touched: impl FnMut(u32),
    ) -> SystemTime {
        match event {
            Event::Gpu => {
                self.gpu.update(
                    &mut self.scheduler,
                    &mut self.int_ctrl,
                    &mut self.timer_ctrl,
                );
                0
            }
            Event::CdRom => {
                self.cdrom.update(&mut self.scheduler, &mut self.int_ctrl);
                0
            }
            Event::Joy => {
                self.joy_bus.update(&mut self.scheduler, &mut self.int_ctrl);
                0
            }
            Event::Dma => DmaController::run(self, ram_touched),
        }
    }

    // Inlined because RAM/BIOS hot paths are needed for caller
    #[inline(always)]
    pub(crate) fn load<const N: usize>(&mut self, paddr: u32) -> [u8; N] {
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
    pub(crate) fn store<const N: usize>(&mut self, paddr: u32, value: [u8; N]) {
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
            Region::Joy => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - JOY_BUS.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "joy bus read");
                self.joy_bus.read_mmio(buf, mmio_addr);
            }
            Region::Int => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - INT_CTRL.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "int ctrl read");
                self.int_ctrl.read_mmio(buf, mmio_addr);
            }
            Region::Dma => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - DMA_CTRL.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "dma ctrl read");
                self.dma_ctrl.read_mmio(buf, mmio_addr);
            }
            Region::Timer => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - TIMER_CTRL.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "timer ctrl read");
                self.timer_ctrl.read_mmio(buf, mmio_addr);
            }
            Region::CdRom => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - CDROM.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "cdrom read");
                self.cdrom.read_mmio(buf, mmio_addr);
            }
            Region::Gpu => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - GPU.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "gpu read");
                self.gpu.read_mmio(buf, mmio_addr);
            }
            Region::Spu => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - SPU.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "spu read");
                self.spu.read_mmio(buf, mmio_addr);
            }
            Region::HwRegs => {
                let _guard = mmio_span.enter();
                tracing::trace!("HW regs touched");
            }
            // Unaligned access is not implemented and *probably* not used anywhere
            Region::Ram => unimplemented!(),
            Region::Bios => unimplemented!(),
            Region::Scratchpad => unimplemented!(),
            Region::Expansion1 | Region::Expansion2 | Region::Unmapped => {}
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
            Region::Joy => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - JOY_BUS.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "joy bus write");
                self.joy_bus.write_mmio(mmio_addr, &value);
            }
            Region::Int => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - INT_CTRL.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "int ctrl write");
                self.int_ctrl.write_mmio(mmio_addr, &value);
            }
            Region::Dma => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - DMA_CTRL.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "dma ctrl write");
                self.dma_ctrl
                    .write_mmio(&mut self.int_ctrl, mmio_addr, &value);
            }
            Region::Timer => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - TIMER_CTRL.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "timer ctrl write");
                self.timer_ctrl
                    .write_mmio(&mut self.scheduler, mmio_addr, &value);
            }
            Region::CdRom => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - CDROM.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "cdrom write");
                self.cdrom
                    .write_mmio(&mut self.scheduler, &mut self.int_ctrl, mmio_addr, &value);
            }
            Region::Gpu => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - GPU.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "gpu write");
                self.gpu
                    .write_mmio(&mut self.scheduler, &mut self.int_ctrl, mmio_addr, &value);
            }
            Region::Spu => {
                let _guard = mmio_span.enter();
                let mmio_addr = paddr - SPU.start;
                tracing::trace!(mmio_addr=%format_args!("{mmio_addr:#X}"), "spu write");
                self.spu.write_mmio(mmio_addr, &value);
            }
            Region::HwRegs => {
                let _guard = mmio_span.enter();
                tracing::trace!("HW regs touched");
            }
            // Unaligned access is not implemented and *probably* not used anywhere
            Region::Ram => unimplemented!(),
            Region::Bios => unimplemented!(),
            Region::Scratchpad => unimplemented!(),
            Region::Expansion1 | Region::Expansion2 | Region::Unmapped => {}
        }
    }
}
