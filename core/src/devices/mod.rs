pub mod dma;
pub mod gpu;
pub mod int;
pub mod timer;

pub trait Mmio {
    fn read(&mut self, dest: &mut [u8], maddr: u32);

    fn write(&mut self, maddr: u32, value: &[u8]);
}

fn read_part<const WINDOW: usize, const SRC: usize>(dest: &mut [u8], addr: u32, src: [u8; SRC]) {
    debug_assert!(matches!(WINDOW, 1 | 2 | 4));
    debug_assert!(matches!(SRC, 1 | 2 | 4));
    debug_assert!(SRC <= WINDOW);
    debug_assert!(dest.len() <= WINDOW);

    let off = (addr as usize) & (WINDOW - 1);

    debug_assert!(off + dest.len() <= WINDOW);

    let mut buf = [0u8; WINDOW];
    buf[..SRC].copy_from_slice(&src);

    dest.copy_from_slice(&buf[off..off + dest.len()]);
}

fn write_part<const WINDOW: usize, const DST: usize>(
    addr: u32,
    value: &[u8],
    old: [u8; DST],
) -> [u8; DST] {
    debug_assert!(matches!(WINDOW, 1 | 2 | 4));
    debug_assert!(matches!(DST, 1 | 2 | 4));
    debug_assert!(DST <= WINDOW);
    debug_assert!(value.len() <= WINDOW);

    let off = (addr as usize) & (WINDOW - 1);

    debug_assert!(off + value.len() <= WINDOW);

    let mut buf = [0u8; WINDOW];
    buf[..DST].copy_from_slice(&old);

    buf[off..off + value.len()].copy_from_slice(value);

    let mut out = [0u8; DST];
    out.copy_from_slice(&buf[..DST]);

    out
}
