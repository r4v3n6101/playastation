use modular_bitfield::prelude::*;

use super::Mmio;

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

#[derive(Debug, Default)]
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
#[derive(Specifier, Debug, Default, Clone, Copy)]
pub struct Bcr {
    /// Count of blocks for [`SyncMode::Request`].
    pub block_count: B16,
    /// Word count/block size, depends on [`SyncMode`]
    pub word_count_or_block_size: B16,
}

/// Channel control register.
#[bitfield(bits = 32)]
#[derive(Specifier, Debug, Default, Clone, Copy)]
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
    pub active: bool,
    #[skip]
    reserved: B3,
    pub trigger: bool,
    #[skip]
    reserved: B4,
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
#[derive(Specifier, Debug, Default, Clone, Copy)]
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
#[derive(Specifier, Debug, Default, Clone, Copy)]
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

/// I really dislike such methods, but arrays aren't supported
impl Dpcr {
    fn chan_enabled(self, ch: usize) -> bool {
        match ch {
            0 => self.enabled0(),
            1 => self.enabled1(),
            2 => self.enabled2(),
            3 => self.enabled3(),
            4 => self.enabled4(),
            5 => self.enabled5(),
            6 => self.enabled6(),
            _ => panic!("only {CHANNELS} channels supported"),
        }
    }

    fn chan_priority(self, ch: usize) -> u8 {
        match ch {
            0 => self.priority0(),
            1 => self.priority1(),
            2 => self.priority2(),
            3 => self.priority3(),
            4 => self.priority4(),
            5 => self.priority5(),
            6 => self.priority6(),
            _ => panic!("only {CHANNELS} channels supported"),
        }
    }
}

impl Dicr {
    pub fn set_irq_lane(&mut self, ch: usize) {
        assert!(ch <= CHANNELS, "only {CHANNELS} supported");

        self.set_irq_flags(1 << ch);
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
        self.chcr.set_trigger(false);
    }

    pub fn try_finish(&mut self) -> bool {
        let finished = match self.chcr.sync_mode() {
            SyncMode::Manual => self.bcr.word_count_or_block_size() == 0,
            SyncMode::Request => self.bcr.block_count() == 0,
            SyncMode::LinkedList => self.madr == 0x00FFFFFF,
            SyncMode::Reserved => panic!("not available"),
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
    pub fn pick_highest_prio_chan(&self) -> Option<usize> {
        let mut best = None;

        for ch in 0..CHANNELS {
            let chan = &self.channels[ch];

            if !self.dpcr.chan_enabled(ch) || !chan.chcr.active() {
                continue;
            }

            match chan.chcr.sync_mode() {
                SyncMode::Manual if !chan.chcr.trigger() => continue,
                // TODO : this is very simplified
                _ => {}
            }

            match best {
                None => best = Some(ch),
                Some(old) => {
                    let p_new = self.dpcr.chan_priority(ch);
                    let p_old = self.dpcr.chan_priority(old);

                    if p_new < p_old || (p_new == p_old && ch > old) {
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
            _ => unimplemented!(),
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
            _ => unimplemented!(),
        }
    }
}
