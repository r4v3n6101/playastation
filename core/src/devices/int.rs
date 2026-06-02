use super::{Mmio, read_part, write_part};

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
        const JOY      = 1 << 7;
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
    fn read(&mut self, dest: &mut [u8], maddr: u32) {
        match maddr {
            0x0..0x4 => {
                read_part::<4, 2>(dest, maddr, self.i_stat.bits().to_le_bytes());
            }
            0x4..0x8 => {
                read_part::<4, 2>(dest, maddr, self.i_mask.bits().to_le_bytes());
            }
            _ => unimplemented!(),
        }
    }

    fn write(&mut self, maddr: u32, value: &[u8]) {
        match maddr {
            0x0..0x4 => {
                self.i_stat &= InterruptFlags::from_bits_truncate(u16::from_le_bytes(
                    write_part::<4, 2>(maddr, value, self.i_stat.bits().to_le_bytes()),
                ));
            }
            0x4..0x8 => {
                self.i_mask =
                    InterruptFlags::from_bits_truncate(u16::from_le_bytes(write_part::<4, 2>(
                        maddr,
                        value,
                        self.i_mask.bits().to_le_bytes(),
                    )));
            }
            _ => unimplemented!(),
        }
    }
}
