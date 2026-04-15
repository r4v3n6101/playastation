use modular_bitfield::prelude::*;
use strum::{EnumCount, EnumIter, FromRepr, IntoEnumIterator};

use super::Mmio;

#[derive(EnumCount, EnumIter, FromRepr, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Port {
    MdecIn = 0,
    MdecOut = 1,
    Gpu = 2,
    CdRom = 3,
    Spu = 4,
    Pio = 5,
    Otc = 6,
}

#[derive(Debug, Default)]
pub struct DmaController {
    /// Channels.
    pub channels: [Channel; Port::COUNT],
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
    pub sync_mode: SyncMode,
    #[skip]
    reserved: B5,
    pub chopping_dma_window: B3,
    #[skip]
    reserved: B1,
    pub chopping_cpu_window: B3,
    #[skip]
    reserved: B1,
    pub active: bool,
    #[skip]
    reserved: B3,
    pub trigger: bool,
    #[skip]
    reserved: B3,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 1]
pub enum Direction {
    ToRam = 0,
    FromRam = 1,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 1]
pub enum Step {
    Increment = 0,
    Decrement = 1,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 2]
pub enum SyncMode {
    Manual = 0,
    Request = 1,
    LinkedList = 2,
    Reserved = 3,
}

/// DMA priority control register.
#[bitfield(bits = 32)]
#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dpcr {
    pub enabled0: bool,
    pub priority0: B3,
    pub enabled1: bool,
    pub priority1: B3,
    pub enabled2: bool,
    pub priority2: B3,
    pub enabled3: bool,
    pub priority3: B3,
    pub enabled4: bool,
    pub priority4: B3,
    pub enabled5: bool,
    pub priority5: B3,
    pub enabled6: bool,
    pub priority6: B3,
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
        const RESET_VAL: u32 = 0x07654321;

        Self::from_bytes(RESET_VAL.to_le_bytes())
    }
}

/// I really dislike such methods, but arrays aren't supported
impl Dpcr {
    fn chan_enabled(self, ch: Port) -> bool {
        match ch {
            Port::MdecIn => self.enabled0(),
            Port::MdecOut => self.enabled1(),
            Port::Gpu => self.enabled2(),
            Port::CdRom => self.enabled3(),
            Port::Spu => self.enabled4(),
            Port::Pio => self.enabled5(),
            Port::Otc => self.enabled6(),
        }
    }

    fn chan_priority(self, ch: Port) -> u8 {
        match ch {
            Port::MdecIn => self.priority0(),
            Port::MdecOut => self.priority1(),
            Port::Gpu => self.priority2(),
            Port::CdRom => self.priority3(),
            Port::Spu => self.priority4(),
            Port::Pio => self.priority5(),
            Port::Otc => self.priority6(),
        }
    }
}

impl Dicr {
    pub fn set_irq_lane(&mut self, ch: Port) {
        self.set_irq_flags(self.irq_flags() | (1 << ch as u8));
        self.update_irq_signal();
    }

    fn update_irq_signal(&mut self) {
        if self.force_irq()
            || (self.master_enabled() && (self.irq_enabled() & self.irq_flags()) != 0)
        {
            self.set_irq_signal(true);
        }
    }
}

impl Channel {
    pub fn start(&mut self) {
        self.chcr.set_active(true);
        if self.chcr.sync_mode() == SyncMode::Manual {
            self.chcr.set_trigger(false);
        }
    }

    pub fn try_finish(&mut self) -> bool {
        let finished = match self.chcr.sync_mode() {
            SyncMode::Manual => self.bcr.word_count() == 0,
            SyncMode::Request => self.bcr.block_count() == 0,
            SyncMode::LinkedList => self.madr == 0x00FFFFFF,
            SyncMode::Reserved => true,
        };

        if finished {
            self.chcr.set_active(false);
        }

        finished
    }
}

impl DmaController {
    /// Pick enabled channel with higher priority
    /// [`Option::None`] if all chans are disabled
    pub fn pick_highest_prio_chan(&self) -> Option<Port> {
        let mut best = None;

        for ch in Port::iter() {
            let chan = &self.channels[ch as usize];

            if !self.dpcr.chan_enabled(ch) || !chan.chcr.active() {
                continue;
            }

            match chan.chcr.sync_mode() {
                SyncMode::Manual if !chan.chcr.trigger() => continue,
                SyncMode::LinkedList if ch != Port::Gpu => continue,
                _ => {}
            }

            match best {
                None => best = Some(ch),
                Some(old) => {
                    let p_old = self.dpcr.chan_priority(old);
                    let p_new = self.dpcr.chan_priority(ch);

                    if p_new <= p_old {
                        best = Some(ch);
                    }
                }
            }
        }

        best
    }
}

impl Mmio for DmaController {
    fn read(&self, dest: &mut [u8], addr: u32) {
        // Word aligned addr
        let reg_addr = addr & !0x3;
        // Position inside a word
        let off = (addr & 0x3) as usize;

        let reg = match reg_addr {
            ..0x70 => {
                let reg = reg_addr % 0x10;
                let chan = (reg_addr / 0x10) as usize;
                match reg {
                    0x0 => self.channels[chan].madr,
                    0x4 => u32::from_le_bytes(self.channels[chan].bcr.into_bytes()),
                    0x8 => u32::from_le_bytes(self.channels[chan].chcr.into_bytes()),
                    _ => unreachable!(),
                }
            }
            0x70 => u32::from_le_bytes(self.dpcr.into_bytes()),
            0x74 => u32::from_le_bytes(self.dicr.into_bytes()),
            _ => unreachable!(),
        };

        let bytes = reg.to_le_bytes();
        dest.copy_from_slice(&bytes[off..][..dest.len()]);
    }

    fn write(&mut self, addr: u32, value: &[u8]) {
        // Word aligned addr
        let reg_addr = addr & !0x3;
        // Position inside a word
        let off = (addr & 0x3) as usize;

        let val = {
            let mut buf = [0u8; 4];
            // Read a word
            self.read(&mut buf, reg_addr);
            // ...then put a value inside the word
            buf[off..off + value.len()].copy_from_slice(value);
            u32::from_le_bytes(buf)
        };
        match reg_addr {
            ..0x70 => {
                let reg = reg_addr % 0x10;
                let chan = (reg_addr / 0x10) as usize;
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

#[cfg(test)]
mod tests {
    use super::{super::Mmio, Bcr, Chcr, Direction, DmaController, Step, SyncMode};

    fn write(ctrl: &mut DmaController, addr: u32, val: u32) {
        ctrl.write(addr, val.to_le_bytes().as_slice());
    }

    #[test]
    fn verify_bios_seq() {
        let mut ctrl = DmaController::default();

        write(&mut ctrl, 0x70, 0x076f4321);
        assert!(ctrl.dpcr.enabled4());
        assert_eq!(ctrl.dpcr.priority4(), 7);

        write(&mut ctrl, 0x74, 0);
        // TODO : what to check?
        //
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

        assert!(ctrl.pick_highest_prio_chan().is_some());
    }
}
