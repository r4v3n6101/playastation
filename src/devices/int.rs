use crate::devices::Mmio;

bitflags::bitflags! {
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct InterruptFlags: u16 {
        const VBLANK   = 1 << 0;
        const GPU      = 1 << 1;
        const CDROM    = 1 << 2;
        const DMA      = 1 << 3;
        const TMR0     = 1 << 4;
        const TMR1     = 1 << 5;
        const TMR2     = 1 << 6;
        const CTRL     = 1 << 7;
        const SIO      = 1 << 8;
        const SPU      = 1 << 9;
        const LIGHTPEN = 1 << 10;
    }
}

#[derive(Debug, Default)]
pub struct InterruptController {
    pub i_stat: InterruptFlags,
    pub i_mask: InterruptFlags,
}

impl InterruptController {
    pub fn pending(&self) -> bool {
        self.i_stat.intersects(self.i_mask)
    }

    pub fn raise(&mut self, int: InterruptFlags) {
        self.i_stat.insert(int);
    }
}

impl Mmio for InterruptController {
    fn read(&self, dest: &mut [u8], addr: u32) {
        // TODO
        let reg = match addr {
            0x1F801070 => self.i_stat.bits(),
            0x1F801074 => self.i_mask.bits(),
            _ => unreachable!(),
        };
    }

    fn write(&mut self, addr: u32, value: &[u8]) {
        // TODO
        match addr {
            0x1F801070 => {
                todo!()
            }
            0x1F801074 => {
                todo!()
            }
            _ => unreachable!(),
        }
    }
}
