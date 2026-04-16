pub mod dma;
pub mod gpu;
pub mod int;

pub trait Mmio {
    fn read(&self, dest: &mut [u8], addr: u32);

    fn write(&mut self, addr: u32, value: &[u8]);
}

trait MmioExt: Mmio {
    fn read_unaligned(&self, dest: &mut [u8], addr: u32, read: impl Fn(u32) -> u32) {
        let (addr, off) = (addr & !0x3, addr & 0x3);
        let val = read(addr);

        let bytes = val.to_le_bytes();
        dest.copy_from_slice(&bytes[off as usize..][..dest.len()]);
    }

    fn write_value(&self, addr: u32, value: &[u8]) -> (u32, u32) {
        let (addr, off) = (addr & !0x3, addr & 0x3);

        let mut buf = [0u8; 4];

        // Read previous value
        self.read(&mut buf, addr);
        // ...then put a value inside the word
        buf[off as usize..][..value.len()].copy_from_slice(value);

        (addr, u32::from_le_bytes(buf))
    }
}

impl<T> MmioExt for T where T: Mmio {}
