// TODO : this is stub to run Silent Hill

#[derive(Debug, Default)]
pub struct Spu {
    // Preserve voice register readback while audio synthesis is unimplemented.
    voices: [[u16; 8]; 24],
    reverb_enable: [u16; 2],
    transfer_start_addr: u16,
    control: u16,
    transfer_control: u16,
}

impl Spu {
    pub(crate) fn read_mmio(&mut self, dest: &mut [u8], maddr: u32) {
        for (offset, byte) in dest.iter_mut().enumerate() {
            let addr = maddr + offset as u32;
            let value = match addr & !1 {
                0x000..=0x17F => self.voices[(addr >> 4) as usize][((addr & 0xF) >> 1) as usize],
                0x198 => self.reverb_enable[0],
                0x19A => self.reverb_enable[1],
                0x1A6 => self.transfer_start_addr,
                0x1AA => self.control,
                0x1AC => self.transfer_control,
                0x1AE => self.control & 0x3F,
                _ => 0,
            };
            *byte = value.to_le_bytes()[(addr & 1) as usize];
        }
    }

    pub(crate) fn write_mmio(&mut self, maddr: u32, value: &[u8]) {
        // TODO: byte writes require the full CPU halfword on the SPU bus.
        for (offset, &bytes) in value.as_chunks::<2>().0.iter().enumerate() {
            let value = u16::from_le_bytes(bytes);
            match maddr + offset as u32 * 2 {
                addr @ 0x000..=0x17F => {
                    self.voices[(addr >> 4) as usize][((addr & 0xF) >> 1) as usize] = value;
                }
                0x198 => self.reverb_enable[0] = value,
                0x19A => self.reverb_enable[1] = value & 0xFF,
                0x1A6 => self.transfer_start_addr = value,
                0x1AA => self.control = value,
                0x1AC => self.transfer_control = value,
                _ => {}
            }
        }
    }
}
