use modular_bitfield::prelude::*;

use super::{Mmio, MmioExt};

#[derive(Debug, Default)]
pub struct Gpu {
    pub gpustat: GpuStat,
}

#[bitfield(bits = 32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuStat {
    pub texture_page_x_base: B4,
    pub texture_page_y_base: bool,
    pub semi_transparency: SemiTransparency,
    pub texture_depth: TextureDepth,
    pub dither_24_to_15: bool,
    pub draw_to_display_area: bool,
    pub set_mask_while_drawing: bool,
    pub draw_to_masked_pixels: bool,
    pub interlace_field: bool,
    pub reverse_flag: bool,
    pub texture_disable: bool,
    pub horizontal_resolution_2: bool,
    pub horizontal_resolution_1: HorizontalResolution,
    pub vertical_resolution: VerticalResolution,
    pub video_mode: VideoMode,
    pub display_depth: DisplayDepth,
    pub vertical_interlace: bool,
    pub display_disabled: bool,
    pub interrupt_request: bool,
    pub dma_data_request: bool,
    pub ready_to_receive_command: bool,
    pub ready_to_send_vram: bool,
    pub ready_to_receive_dma: bool,
    pub dma_direction: GpuDmaDirection,
    pub drawing_even_odd_lines: bool,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 2]
pub enum SemiTransparency {
    Average = 0,
    Add = 1,
    Subtract = 2,
    AddQuarter = 3,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 2]
pub enum TextureDepth {
    Bpp4 = 0,
    Bpp8 = 1,
    Bpp15 = 2,
    Reserved = 3,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 2]
pub enum HorizontalResolution {
    H256 = 0,
    H320 = 1,
    H512 = 2,
    H640 = 3,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 1]
pub enum VerticalResolution {
    V240 = 0,
    V480 = 1,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 1]
pub enum VideoMode {
    Ntsc = 0,
    Pal = 1,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 1]
pub enum DisplayDepth {
    Bpp15 = 0,
    Bpp24 = 1,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 2]
pub enum GpuDmaDirection {
    Off = 0,
    Fifo = 1,
    CpuToGp0 = 2,
    VramToCpu = 3,
}

impl Default for GpuStat {
    fn default() -> Self {
        Self::new()
            .with_ready_to_receive_command(true)
            .with_ready_to_send_vram(true)
            .with_ready_to_receive_dma(true)
    }
}

impl Mmio for Gpu {
    fn read(&self, dest: &mut [u8], addr: u32) {
        self.read_unaligned(dest, addr, |addr| match addr {
            0x0 => 0,
            0x4 => u32::from_le_bytes(self.gpustat.into_bytes()),
            _ => unreachable!(),
        });
    }

    fn write(&mut self, addr: u32, value: &[u8]) {
        dbg!("gp0/1 commands", addr, value);
    }
}

#[cfg(test)]
mod tests {
    use super::GpuStat;

    #[test]
    fn verify_default() {
        let gpustat = GpuStat::default();
        let reg = u32::from_le_bytes(gpustat.into_bytes());

        assert_eq!(reg, 0x1C000000);
    }
}
