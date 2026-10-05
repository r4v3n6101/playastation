use derive_more::Debug;
use modular_bitfield::prelude::*;

use crate::{
    SystemTime,
    devices::int::{InterruptController, InterruptFlags},
    interconnect::{bus::Bus, scheduler::Event},
};

use super::{read_part, write_part};

mod handler;

const POLL_INTERVAL: SystemTime = 128;

const CHANNELS: usize = 7;

#[derive(Debug, Default)]
pub struct DmaController {
    /// Channels.
    pub channels: [Channel; CHANNELS],
    /// Control / priority.
    pub dpcr: Dpcr,
    /// Interrupt control.
    pub dicr: Dicr,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Channel {
    /// Memory address.
    #[debug("{madr:#X}")]
    pub madr: u32,
    /// Block control.
    pub bcr: Bcr,
    /// Channel control.
    pub chcr: Chcr,
}

/// Block control register.
#[bitfield(bits = 32)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Bcr {
    /// Word count/block size (in words), depends on [`SyncMode`]
    pub word_count: B16,
    /// Count of blocks for [`SyncMode::Request`].
    pub block_count: B16,
}

/// Channel control register.
#[bitfield(bits = 32)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Chcr {
    pub direction: Direction,
    pub step: Step,
    #[skip]
    reserved: B6,
    pub chopping_enabled: bool,
    /// Sync mode for choosing transfer type.
    pub sync_mode: SyncMode,
    #[skip]
    reserved: B5,
    pub chopping_dma_window: B3,
    #[skip]
    reserved: B1,
    pub chopping_cpu_window: B3,
    #[skip]
    reserved: B1,
    /// Start/busy.
    pub active: bool,
    #[skip]
    reserved: B3,
    /// Trigger for [`SyncMode::Manual`]
    pub trigger: bool,
    #[skip]
    reserved: B3,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 1]
pub enum Direction {
    /// Copy from a device to RAM.
    ToRam = 0,
    /// Copy from RAM to a device.
    FromRam = 1,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 1]
pub enum Step {
    /// Address goes forward (+4).
    Increment = 0,
    /// Address foes backward (-4).
    Decrement = 1,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 2]
pub enum SyncMode {
    /// Word-by-word transfer, with `trigger` bit.
    Manual = 0,
    /// Block transfer, `trigger` bit isn't used.
    Request = 1,
    /// Linked list works only with GPU (channel = 2).
    LinkedList = 2,
    /// Unused.
    Reserved = 3,
}

/// DMA priority control register.
#[bitfield(bits = 32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dpcr {
    pub priority0: B3,
    pub enabled0: bool,
    pub priority1: B3,
    pub enabled1: bool,
    pub priority2: B3,
    pub enabled2: bool,
    pub priority3: B3,
    pub enabled3: bool,
    pub priority4: B3,
    pub enabled4: bool,
    pub priority5: B3,
    pub enabled5: bool,
    pub priority6: B3,
    pub enabled6: bool,
    #[skip]
    reserved: B4,
}

/// DMA interrupt controller register.
#[bitfield(bits = 32)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Dicr {
    #[skip]
    reserved: B6,
    #[skip]
    reserved: B9,
    /// Force raising IRQ
    pub force_irq: bool,
    /// Enabled interrupts lanes.
    pub irq_enabled: B7,
    /// Master flag for all interrupts.
    pub master_enabled: bool,
    /// Pending interrupts.
    pub irq_flags: B7,
    // Bit 31 is computed when reading DICR.
    #[skip(getters)]
    irq_signal: bool,
}

impl Default for Dpcr {
    fn default() -> Self {
        Self::new()
            .with_priority0(1)
            .with_priority1(2)
            .with_priority2(3)
            .with_priority3(4)
            .with_priority4(5)
            .with_priority5(6)
            .with_priority6(7)
    }
}

impl Dicr {
    fn irq_signal(&self) -> bool {
        self.force_irq() || (self.master_enabled() && self.irq_flags() != 0)
    }

    fn set_irq_lane(&mut self, ch: usize) {
        if self.master_enabled() && self.irq_enabled() & (1 << ch) != 0 {
            self.set_irq_flags(self.irq_flags() | (1 << ch));
        }
    }
}

impl DmaController {
    pub(crate) fn run(bus: &mut Bus, mut ram_touched: impl FnMut(u32)) -> SystemTime {
        let mut duration: SystemTime = 0;

        if let Some(ch) = bus.dma_ctrl.pick_highest_priority_chan() {
            let mut chan = bus.dma_ctrl.channels[ch];

            // trigger bit must be present when sync_mode is Manual
            if !matches!(chan.chcr.sync_mode(), SyncMode::Manual) || chan.chcr.trigger() {
                {
                    let transfer_span = tracing::debug_span!(
                        target: "dma",
                        "transfer",
                        index=%ch,
                        ?chan
                    );
                    let _guard = transfer_span.enter();

                    let elapsed = match chan.chcr.sync_mode() {
                        SyncMode::Manual => {
                            handler::do_manual(bus, ch, &mut chan, &mut ram_touched)
                        }
                        SyncMode::Request => {
                            handler::do_block(bus, ch, &mut chan, &mut ram_touched)
                        }
                        SyncMode::LinkedList => handler::do_linked_list(bus, ch, &mut chan),
                        SyncMode::Reserved => unreachable!(),
                    };
                    tracing::trace!(%elapsed, "dma transfer completed");
                    duration = duration.saturating_add(elapsed);
                }

                chan.chcr.set_active(false);
                chan.chcr.set_trigger(false);

                let was_asserted = bus.dma_ctrl.dicr.irq_signal();
                bus.dma_ctrl.dicr.set_irq_lane(ch);
                if !was_asserted && bus.dma_ctrl.dicr.irq_signal() {
                    bus.int_ctrl.raise(InterruptFlags::DMA);
                }

                bus.dma_ctrl.channels[ch] = chan;
            }
        }

        bus.scheduler.schedule(Event::Dma, duration + POLL_INTERVAL);

        duration
    }

    pub(crate) fn read_mmio(&mut self, dest: &mut [u8], maddr: u32) {
        match maddr {
            ..0x70 => {
                let reg = maddr % 0x10;
                let chan = (maddr / 0x10) as usize;
                match reg {
                    ..0x4 => {
                        read_part::<4, 4>(dest, maddr, self.channels[chan].madr.to_le_bytes());
                    }
                    0x4..0x8 => {
                        read_part::<4, 4>(dest, maddr, self.channels[chan].bcr.into_bytes());
                    }
                    // It is mirror, don't want to merge ranges for clarity
                    #[allow(clippy::manual_range_patterns)]
                    0x8..0xC | 0xC..0x10 => {
                        read_part::<4, 4>(dest, maddr, self.channels[chan].chcr.into_bytes());
                    }
                    _ => unreachable!(),
                }
            }
            0x70..0x74 => {
                read_part::<4, 4>(dest, maddr, self.dpcr.into_bytes());
            }
            0x74..0x78 => {
                let value = self.dicr.with_irq_signal(self.dicr.irq_signal());
                read_part::<4, 4>(dest, maddr, value.into_bytes());
            }
            _ => unimplemented!(),
        }
    }

    pub(crate) fn write_mmio(
        &mut self,
        int_ctrl: &mut InterruptController,
        maddr: u32,
        value: &[u8],
    ) {
        match maddr {
            ..0x70 => {
                let reg = maddr % 0x10;
                let chan = (maddr / 0x10) as usize;
                match reg {
                    0x0..0x4 => {
                        self.channels[chan].madr = u32::from_le_bytes(write_part::<4, 4>(
                            maddr,
                            value,
                            self.channels[chan].madr.to_le_bytes(),
                        ));
                    }
                    0x4..0x8 => {
                        self.channels[chan].bcr = Bcr::from_bytes(write_part::<4, 4>(
                            maddr,
                            value,
                            self.channels[chan].bcr.into_bytes(),
                        ));
                    }
                    // same as above
                    #[allow(clippy::manual_range_patterns)]
                    0x8..0xC | 0xC..0x10 => {
                        self.channels[chan].chcr = Chcr::from_bytes(write_part::<4, 4>(
                            maddr,
                            value,
                            self.channels[chan].chcr.into_bytes(),
                        ));
                    }
                    _ => unreachable!(),
                }
            }
            0x70..0x74 => {
                self.dpcr =
                    Dpcr::from_bytes(write_part::<4, 4>(maddr, value, self.dpcr.into_bytes()));
            }
            0x74..0x78 => {
                let was_asserted = self.dicr.irq_signal();
                let new =
                    Dicr::from_bytes(write_part::<4, 4>(maddr, value, self.dicr.into_bytes()));

                self.dicr.set_force_irq(new.force_irq());
                self.dicr.set_irq_enabled(new.irq_enabled());
                self.dicr.set_master_enabled(new.master_enabled());

                // W1C on bits 24..30: writing 1 clears the corresponding existing flag bit
                let ack = Dicr::from_bytes(write_part::<4, 4>(maddr, value, [0; 4]));
                self.dicr
                    .set_irq_flags(self.dicr.irq_flags() & !ack.irq_flags());

                if !was_asserted && self.dicr.irq_signal() {
                    int_ctrl.raise(InterruptFlags::DMA);
                }
            }
            _ => unimplemented!(),
        }
    }

    fn pick_highest_priority_chan(&self) -> Option<usize> {
        fn dma_prio(dpcr: u32, ch: usize) -> u8 {
            ((dpcr >> (ch * 4)) & 0x7) as u8
        }
        fn dma_enabled(dpcr: u32, ch: usize) -> bool {
            ((dpcr >> (ch * 4 + 3)) & 1) != 0
        }

        let mut best = None;
        let mut best_prio = u8::MAX;

        let dpcr = u32::from_le_bytes(self.dpcr.into_bytes());
        for ch in 0..7 {
            if !dma_enabled(dpcr, ch) {
                continue;
            }

            if !self.channels[ch].chcr.active() {
                continue;
            }

            let prio = dma_prio(dpcr, ch);
            if prio < best_prio {
                best = Some(ch);
                best_prio = prio;
            }
        }

        best
    }
}
