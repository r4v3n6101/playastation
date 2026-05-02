use std::array;

use modular_bitfield::prelude::*;

use crate::{devices::Updater, interconnect::Bus};

use super::{Mmio, MmioExt};

mod handler;

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
    pub madr: u32,
    /// Block control.
    pub bcr: Bcr,
    /// Channel control.
    pub chcr: Chcr,
}

/// Block control register.
#[bitfield(bits = 32)]
#[derive(Specifier, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Bcr {
    /// Word count/block size (in words), depends on [`SyncMode`]
    pub word_count: B16,
    /// Count of blocks for [`SyncMode::Request`].
    pub block_count: B16,
}

/// Channel control register.
#[bitfield(bits = 32)]
#[derive(Specifier, Debug, Default, Clone, Copy, PartialEq, Eq)]
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
#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Specifier, Debug, Default, Clone, Copy, PartialEq, Eq)]
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
    /// Pending intrrupt controller call.
    pub irq_signal: bool,
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

impl Dpcr {
    fn sorted_chans(self) -> [(usize, bool); CHANNELS] {
        let chan_enabled = |ch: usize| match ch {
            0 => self.enabled0(),
            1 => self.enabled1(),
            2 => self.enabled2(),
            3 => self.enabled3(),
            4 => self.enabled4(),
            5 => self.enabled5(),
            6 => self.enabled6(),
            _ => unreachable!(),
        };
        let chan_prio = |ch: usize| match ch {
            0 => self.priority0(),
            1 => self.priority1(),
            2 => self.priority2(),
            3 => self.priority3(),
            4 => self.priority4(),
            5 => self.priority5(),
            6 => self.priority6(),
            _ => unreachable!(),
        };
        let mut channels = array::from_fn(|x| (x, chan_enabled(x)));

        // Lower prio goes first, if equal then higher index go first
        channels.sort_unstable_by(|(idx1, _), (idx2, _)| {
            chan_prio(*idx1)
                .cmp(&chan_prio(*idx2))
                .then(idx1.cmp(idx2).reverse())
        });

        channels
    }
}

impl Dicr {
    fn set_irq_lane(&mut self, ch: usize) {
        self.set_irq_flags(self.irq_flags() | (1 << ch));
        self.update_irq_signal();
    }

    fn update_irq_signal(&mut self) {
        self.set_irq_signal(
            self.force_irq()
                || (self.master_enabled() && (self.irq_enabled() & self.irq_flags()) != 0),
        );
    }
}

impl Mmio for DmaController {
    fn read(&mut self, dest: &mut [u8], addr: u32) {
        self.read_unaligned(dest, addr, |this, addr| match addr {
            ..0x70 => {
                let reg = addr % 0x10;
                let chan = (addr / 0x10) as usize;
                match reg {
                    0x0 => this.channels[chan].madr,
                    0x4 => u32::from_le_bytes(this.channels[chan].bcr.into_bytes()),
                    0x8 => u32::from_le_bytes(this.channels[chan].chcr.into_bytes()),
                    _ => unreachable!(),
                }
            }
            0x70 => u32::from_le_bytes(this.dpcr.into_bytes()),
            0x74 => u32::from_le_bytes(this.dicr.into_bytes()),
            _ => unreachable!(),
        });
    }

    fn write(&mut self, addr: u32, value: &[u8]) {
        let (addr, val) = self.write_value(addr, value);
        match addr {
            ..0x70 => {
                let reg = addr % 0x10;
                let chan = (addr / 0x10) as usize;
                match reg {
                    0x0 => self.channels[chan].madr = val,
                    0x4 => self.channels[chan].bcr = Bcr::from_bytes(val.to_le_bytes()),
                    0x8 => self.channels[chan].chcr = Chcr::from_bytes(val.to_le_bytes()),
                    _ => unreachable!(),
                }
            }
            0x70 => self.dpcr = Dpcr::from_bytes(val.to_le_bytes()),
            0x74 => {
                let new = Dicr::from_bytes(val.to_le_bytes());

                self.dicr.set_force_irq(new.force_irq());
                self.dicr.set_irq_enabled(new.irq_enabled());
                self.dicr.set_master_enabled(new.master_enabled());

                // W1C on bits 24..30: writing 1 clears the corresponding existing flag bit
                self.dicr
                    .set_irq_flags(self.dicr.irq_flags() & !new.irq_flags());

                // Recalculate irq signal bit, rather than copy it
                self.dicr.update_irq_signal();
            }
            _ => unreachable!(),
        }
    }
}

impl Updater for DmaController {
    fn tick(bus: &mut Bus) {
        for (ch, enabled) in bus.dma_ctrl.dpcr.sorted_chans() {
            if !enabled {
                continue;
            }

            let mut chan = bus.dma_ctrl.channels[ch];

            // trigger bit must be present when sync_mode is Manual
            if matches!(chan.chcr.sync_mode(), SyncMode::Manual) && !chan.chcr.trigger() {
                continue;
            }

            // non-active skipped at all
            // TODO : should be for trigger = true, active = false?
            if !chan.chcr.active() {
                continue;
            }

            match chan.chcr.sync_mode() {
                SyncMode::Manual => handler::do_manual(bus, ch, &mut chan),
                SyncMode::Request => handler::do_block(bus, ch, &mut chan),
                SyncMode::LinkedList => handler::do_linked_list(bus, ch, &mut chan),
                SyncMode::Reserved => unreachable!(),
            }

            chan.chcr.set_active(false);
            chan.chcr.set_trigger(false);

            // TODO
            // bus.dma_ctrl.dicr.set_irq_lane(ch);
            // if bus.dma_ctrl.dicr.irq_signal() {
            //     bus.int_ctrl.raise(InterruptFlags::DMA);
            // }

            bus.dma_ctrl.channels[ch] = chan;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{super::Mmio, Bcr, Chcr, Direction, DmaController, Dpcr, Step, SyncMode};

    fn write(ctrl: &mut DmaController, addr: u32, val: u32) {
        ctrl.write(addr, val.to_le_bytes().as_slice());
    }

    #[test]
    fn verify_default() {
        let dpcr = Dpcr::default();
        let reg = u32::from_le_bytes(dpcr.into_bytes());

        assert_eq!(reg, 0x07654321);
    }

    #[test]
    fn verify_bios_seq() {
        let mut ctrl = DmaController::default();

        write(&mut ctrl, 0x70, 0x076f4321);
        assert!(ctrl.dpcr.enabled4());
        assert_eq!(ctrl.dpcr.priority4(), 7);

        write(&mut ctrl, 0x74, 0);
        write(&mut ctrl, 0x74, 0x4840000);

        write(&mut ctrl, 0x28, 0x401);
        assert_eq!(
            ctrl.channels[2].chcr,
            Chcr::new()
                .with_sync_mode(SyncMode::LinkedList)
                .with_direction(Direction::FromRam)
        );

        write(&mut ctrl, 0x70, 0xf6f4321);

        write(&mut ctrl, 0x60, 0x800eb8d4);
        assert_eq!(ctrl.channels[6].madr, 0x800eb8d4);

        write(&mut ctrl, 0x64, 0x00000400);
        assert_eq!(ctrl.channels[6].bcr, Bcr::new().with_word_count(1024));

        write(&mut ctrl, 0x68, 0x11000002);
        assert_eq!(
            ctrl.channels[6].chcr,
            Chcr::new()
                .with_sync_mode(SyncMode::Manual)
                .with_direction(Direction::ToRam)
                .with_step(Step::Decrement)
                .with_active(true)
                .with_trigger(true)
        );
    }
}
