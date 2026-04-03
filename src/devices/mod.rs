pub mod dma;
pub mod int;

pub trait Mmio {
    fn read(&self, dest: &mut [u8], addr: u32);

    fn write(&mut self, addr: u32, value: &[u8]);
}
