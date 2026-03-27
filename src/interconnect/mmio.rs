use std::ops::Range;

// Hardware registers regions
const IRQ_CTRL: Range<u32> = 0x1F801070..0x1F801078;

#[derive(Default)]
pub struct Mmio {}

impl Mmio {
    pub fn read(&self, dest: &mut [u8], addr: u32) {
        todo!()
    }

    pub fn write(&self, addr: u32, value: &[u8]) {
        todo!()
    }
}
