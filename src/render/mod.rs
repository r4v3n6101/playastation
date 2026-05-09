pub mod types;

pub trait GpuBackend: 'static {
    // TODO : issue a command with drawing primitives

    /// Blit VRAM area to the local storage. This is done just before VRAM <=> CPU transfer.
    fn take_vram_area_snapshot(&mut self, pos: types::Position, size: types::Size);

    /// Commit local VRAM snapshot back to GPU backend.
    /// Work with area, so the texture won't be overwritten fully, so not cause VRAM incoherensy.
    fn commit_vram_snapshot(&self);

    /// Push a pixel into the VRAM snapshot taken via [`Self::take_vram_area_snapshot`],
    /// and increment pointer to the next pixel to be written.
    fn push_snapshot_pixel(&mut self, pixel: u16);

    /// Read a pixel from VRAM snapshot taken via [`Self::take_vram_area_snapshot`]
    /// and move pointer forward to the next pixel (if any have left in the area of snapshot).
    fn pop_snapshot_pixel(&mut self) -> Option<u16>;

    /// Reset state like push/pop counters, etc.
    fn reset(&mut self);
}
