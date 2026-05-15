use super::Mmio;

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

    pub fn clear(&mut self, int: InterruptFlags) {
        self.i_stat.remove(int);
    }
}

impl Mmio for InterruptController {
    fn read(&mut self, dest: &mut [u8], addr: u32) {
        match (addr, dest.len()) {
            (0x0, 4) => {
                dest.copy_from_slice(&u32::from(self.i_stat.bits()).to_le_bytes());
            }
            (0x4, 4) => {
                dest.copy_from_slice(&u32::from(self.i_mask.bits()).to_le_bytes());
            }

            (0x0, 2) => {
                dest.copy_from_slice(&self.i_stat.bits().to_le_bytes());
            }
            (0x4, 2) => {
                dest.copy_from_slice(&self.i_mask.bits().to_le_bytes());
            }

            (0x0, 1) => {}
            (0x4, 1) => {}

            _ => unimplemented!(),
        }
    }

    fn write(&mut self, addr: u32, value: &[u8]) {
        match (addr, value.len()) {
            (0x0, 4) => {
                self.i_stat &=
                    InterruptFlags::from_bits_truncate(u16::from_le_bytes([value[2], value[3]]));
            }
            (0x4, 4) => {
                self.i_mask =
                    InterruptFlags::from_bits_truncate(u16::from_le_bytes([value[2], value[3]]));
            }

            (0x0, 2) => {
                self.i_stat &=
                    InterruptFlags::from_bits_truncate(u16::from_le_bytes([value[0], value[1]]));
            }
            (0x4, 2) => {
                self.i_mask =
                    InterruptFlags::from_bits_truncate(u16::from_le_bytes([value[0], value[1]]));
            }

            (0x0, 1) => {}
            (0x4, 1) => {}

            _ => unreachable!(),
        }
    }
}
