use modular_bitfield::prelude::*;

use crate::{devices::Updater, interconnect::Bus};

use super::{Mmio, MmioExt};

mod gp0;
mod gp1;
mod types;

type Vram = Box<[[u16; VRAM_WIDTH]; VRAM_HEIGHT]>;

const VRAM_WIDTH: usize = 1024;
const VRAM_HEIGHT: usize = 512;

#[derive(Debug)]
pub struct Gpu {
    pub gpustat: GpuStat,
    pub vram: Vram,

    cmdbuf: gp0::CmdBuf,
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
            .with_interlace_field(true)
            .with_display_disabled(true)
            .with_ready_to_receive_command(true)
            .with_ready_to_receive_dma(true)
    }
}

impl Default for Gpu {
    fn default() -> Self {
        Self {
            gpustat: GpuStat::default(),
            vram: Box::new([[0; _]; _]),

            cmdbuf: gp0::CmdBuf::default(),
        }
    }
}

impl Gpu {
    pub fn dispatch_gp0(&mut self, cmd: u32) {
        self.cmdbuf.push_cmd(cmd, &mut self.vram);
    }

    pub fn dispatch_gp1(&mut self, cmd: u32) {
        gp1::process(self, cmd);
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
        let (addr, value) = self.write_value(addr, value);
        match addr {
            0x0 => self.dispatch_gp0(value),
            0x4 => self.dispatch_gp1(value),
            _ => unreachable!(),
        }
    }
}

impl Updater for Gpu {
    fn tick(bus: &mut Bus) {
        // if bus.gpu.gpustat.interrupt_request() {
        //     bus.int_ctrl.raise(InterruptFlags::GPU);
        // }
    }
}

#[cfg(test)]
mod tests {
    use super::GpuStat;

    #[test]
    fn verify_default() {
        let gpustat = GpuStat::default();
        let reg = u32::from_le_bytes(gpustat.into_bytes());

        assert_eq!(reg, 0x1480_2000);
    }
}
