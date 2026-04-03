use super::Mmio;

bitfield::bitfield! {
    #[derive(Default, Clone, Copy)]
    pub struct Dpcr(u32);
    impl Debug;

    // Channel enables
    pub ch0_enable, set_ch0_enable: 0;
    pub ch1_enable, set_ch1_enable: 4;
    pub ch2_enable, set_ch2_enable: 8;
    pub ch3_enable, set_ch3_enable: 12;
    pub ch4_enable, set_ch4_enable: 16;
    pub ch5_enable, set_ch5_enable: 20;
    pub ch6_enable, set_ch6_enable: 24;

    // Channel priorities (3 bits each)
    pub ch0_priority, set_ch0_priority: 3, 1;
    pub ch1_priority, set_ch1_priority: 7, 5;
    pub ch2_priority, set_ch2_priority: 11, 9;
    pub ch3_priority, set_ch3_priority: 15, 13;
    pub ch4_priority, set_ch4_priority: 19, 17;
    pub ch5_priority, set_ch5_priority: 23, 21;
    pub ch6_priority, set_ch6_priority: 27, 25;
}

bitfield::bitfield! {
    #[derive(Default, Clone, Copy)]
    pub struct Dicr(u32);
    impl Debug;

    pub unknown_lo, set_unknown_lo: 5, 0;
    pub force_irq, set_force_irq: 15;
    pub irq_enable, set_irq_enable: 22, 16;
    pub master_enable, set_master_enable: 23;
    pub irq_flags, set_irq_flags: 30, 24;
    pub irq_signal, set_irq_signal: 31;
}

#[derive(Debug, Default)]
pub struct DmaController {
    /// Channels.
    pub channels: [DmaChannel; 7],
    /// Control / priority.
    pub dpcr: Dpcr,
    /// Interrupt control.
    pub dicr: Dicr,
}

#[derive(Debug, Default)]
pub struct DmaChannel {
    /// Memory address.
    pub madr: u32,
    /// Block control.
    pub bcr: u32,
    /// Channel control.
    pub chcr: u32,
}

impl DmaChannel {
    pub fn done(&mut self) {
        self.chcr &= !(1 << 24);
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
                    0x4 => self.channels[chan].bcr,
                    0x8 => self.channels[chan].chcr,
                    _ => unreachable!(),
                }
            }
            0x70 => self.dpcr.0,
            0x74 => self.dicr.0,
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
                    0x4 => self.channels[chan].bcr = val,
                    0x8 => self.channels[chan].chcr = val,
                    _ => unreachable!(),
                }
            }
            0x70 => self.dpcr.0 = val,
            0x74 => write_dicr(&mut self.dicr, val),
            _ => unimplemented!(),
        };
    }
}

fn write_dicr(reg: &mut Dicr, new: u32) {
    let new = Dicr(new);

    // Plain writable fields
    reg.set_unknown_lo(new.unknown_lo());
    reg.set_force_irq(new.force_irq());
    reg.set_irq_enable(new.irq_enable());
    reg.set_master_enable(new.master_enable());

    // W1C on bits 24..30: writing 1 clears the corresponding existing flag bit
    reg.set_irq_flags(reg.irq_flags() & !new.irq_flags());

    // Recompute bit31 instead of writing it directly
    reg.set_irq_signal(reg.force_irq() || (reg.master_enable() && reg.irq_flags() != 0));
}
