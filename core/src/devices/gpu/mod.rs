use alloc::boxed::Box;

use modular_bitfield::prelude::*;

use crate::{
    devices::int::InterruptFlags,
    interconnect::Bus,
    render::{
        Renderer,
        noop::NoopRenderer,
        types::{
            DisplayDepth, DmaDirection, HorizontalResolution, RenderState, SemiTransparency,
            TextureDepth, VerticalResolution, VideoMode,
        },
    },
};

use super::{Mmio, MmioExt};

mod gp0;
mod gp1;

pub struct Gpu {
    pub renderer: Box<dyn Renderer>,

    cmdbuf: gp0::CmdBuf,
    int_flag: bool,
    dma_direction: DmaDirection,

    cycles_elapsed: u64,
}

#[bitfield(bits = 32)]
#[derive(Debug, Clone)]
pub struct GpuStat {
    pub texture_page_x_base: B4,
    pub texture_page_y_base: B1,
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
    pub dma_direction: DmaDirection,
    pub drawing_even_odd_lines: bool,
}

impl Default for Gpu {
    fn default() -> Self {
        Self {
            renderer: Box::new(NoopRenderer::default()),
            int_flag: false,

            cycles_elapsed: 0,
            dma_direction: DmaDirection::CpuToGp0,
            cmdbuf: gp0::CmdBuf::default(),
        }
    }
}

impl Gpu {
    pub fn gpustat(&self) -> GpuStat {
        let RenderState {
            draw_mode,
            vram_read_active,
            mask_bit_setting,
        } = self.renderer.state();

        let ready_to_receive_command = true;
        let ready_to_receive_dma = true;
        let ready_to_send_vram = vram_read_active;
        let dma_data_request = match self.dma_direction {
            DmaDirection::Off => false,
            DmaDirection::Fifo => ready_to_receive_command,
            DmaDirection::CpuToGp0 => ready_to_receive_dma,
            DmaDirection::VramToCpu => ready_to_send_vram,
        };

        GpuStat::new()
            // Via [`DrawMode`]
            .with_texture_page_x_base(draw_mode.tex_page().texture_page_x_base())
            .with_texture_page_y_base(draw_mode.tex_page().texture_page_y_base())
            .with_semi_transparency(draw_mode.tex_page().semi_transparency())
            .with_texture_depth(draw_mode.tex_page().texture_depth())
            .with_dither_24_to_15(draw_mode.dither_24_to_15())
            .with_draw_to_display_area(draw_mode.draw_to_display_area())
            .with_texture_disable(draw_mode.texture_disable())
            // Via [`MaskBitSetting`]
            .with_set_mask_while_drawing(mask_bit_setting.set_mask_while_drawing())
            .with_draw_to_masked_pixels(mask_bit_setting.draw_to_masked_pixels())
            // Other
            .with_interrupt_request(self.int_flag)
            .with_interlace_field(true)
            .with_display_disabled(true)
            // DMA related
            .with_dma_direction(self.dma_direction)
            .with_ready_to_receive_command(ready_to_receive_command)
            .with_ready_to_send_vram(ready_to_send_vram)
            .with_ready_to_receive_dma(ready_to_receive_dma)
            .with_dma_data_request(dma_data_request)
    }

    pub fn dispatch_gp0(&mut self, cmd: u32) {
        gp0::dispatch(self, cmd);
    }

    pub fn dispatch_gp1(&mut self, cmd: u32) {
        gp1::dispatch(self, cmd);
    }

    pub fn run(bus: &mut Bus, sys_cycles: u64) {
        bus.gpu.cycles_elapsed = bus.gpu.cycles_elapsed.saturating_add(sys_cycles);

        if bus.gpu.int_flag {
            bus.int_ctrl.raise(InterruptFlags::GPU);
        }

        // TODO : FPS
        let frame_cycles = 33_000_000 / 60;
        while bus.gpu.cycles_elapsed > frame_cycles {
            bus.gpu.renderer.draw_frame();
            bus.int_ctrl.raise(InterruptFlags::VBLANK);
            bus.gpu.cycles_elapsed -= frame_cycles;
        }
    }
}

impl Mmio for Gpu {
    fn read(&mut self, dest: &mut [u8], maddr: u32) {
        self.read_unaligned(dest, maddr, |this, addr| match addr {
            0x0 => gp0::read(this),
            0x4 => u32::from_le_bytes(this.gpustat().into_bytes()),
            _ => unreachable!(),
        });
    }

    fn write(&mut self, maddr: u32, value: &[u8]) {
        let (addr, value) = self.write_unaligned(maddr, value);
        match addr {
            0x0 => self.dispatch_gp0(value),
            0x4 => self.dispatch_gp1(value),
            _ => unreachable!(),
        }
    }
}
