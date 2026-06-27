use alloc::boxed::Box;

use modular_bitfield::prelude::*;

use crate::{
    devices::{
        int::{InterruptController, InterruptFlags},
        timer::{TimingEvent, TimingSpan},
    },
    render::{
        Renderer,
        noop::NoopRenderer,
        types::{RenderState, SemiTransparency, TextureDepth},
    },
};

use super::{Mmio, read_part, write_part};

mod clock;
mod gp0;
mod gp1;

#[derive(Default)]
struct Display {
    hres: HorizontalResolution,
    vres: VerticalResolution,
    vmode: VideoMode,
    display_depth: DisplayDepth,
    interlaced: bool,
    special_368_hres: bool,
    reversed: bool,
    enabled: bool,
}

pub struct Gpu {
    // Renderer and all state of it (like masks, textures)
    pub renderer: Box<dyn Renderer>,
    /// Start coordinate in VRAM.
    pub vram_start: (u16, u16),
    /// Horizontal range, may differ from resolution.
    pub hrange: (u16, u16),
    /// Same as above, but vertical.
    pub vrange: (u16, u16),

    // Inner modules
    clock: clock::State,
    cmdbuf: gp0::CmdBuf,

    // GPU state itself
    draw_odd_even_frame: bool,
    dma_direction: DmaDirection,
    display: Display,

    int_flag: bool,
}

#[bitfield(bits = 32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub special_hres_368: bool,
    pub hres: HorizontalResolution,
    pub vres: VerticalResolution,
    pub vmode: VideoMode,
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

#[derive(Specifier, Debug, Default, Clone, Copy, PartialEq, Eq)]
#[bits = 2]
pub enum HorizontalResolution {
    #[default]
    H256 = 0,
    H320 = 1,
    H512 = 2,
    H640 = 3,
}

#[derive(Specifier, Debug, Default, Clone, Copy, PartialEq, Eq)]
#[bits = 1]
pub enum VerticalResolution {
    #[default]
    V240 = 0,
    V480 = 1,
}

#[derive(Specifier, Debug, Default, Clone, Copy, PartialEq, Eq)]
#[bits = 1]
pub enum VideoMode {
    #[default]
    Ntsc = 0,
    Pal = 1,
}

#[derive(Specifier, Debug, Default, Clone, Copy, PartialEq, Eq)]
#[bits = 1]
pub enum DisplayDepth {
    #[default]
    Bpp15 = 0,
    Bpp24 = 1,
}

#[derive(Specifier, Debug, Default, Clone, Copy, PartialEq, Eq)]
#[bits = 2]
pub enum DmaDirection {
    #[default]
    Off = 0,
    Fifo = 1,
    CpuToGp0 = 2,
    VramToCpu = 3,
}

impl Default for Gpu {
    fn default() -> Self {
        Self {
            renderer: Box::new(NoopRenderer::default()),
            vram_start: (0, 0),
            hrange: (0, 0),
            vrange: (0, 0),

            clock: clock::State::default(),
            cmdbuf: gp0::CmdBuf::default(),

            draw_odd_even_frame: false,
            dma_direction: DmaDirection::default(),
            display: Display::default(),

            int_flag: false,
        }
    }
}

impl Gpu {
    pub fn stat(&self) -> GpuStat {
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
            // Display info
            .with_hres(self.display.hres)
            .with_vres(self.display.vres)
            .with_vmode(self.display.vmode)
            .with_display_depth(self.display.display_depth)
            .with_interlace_field(self.display.interlaced)
            .with_special_hres_368(self.display.special_368_hres)
            .with_reverse_flag(self.display.reversed)
            // Via [`MaskBitSetting`]
            .with_set_mask_while_drawing(mask_bit_setting.set_mask_while_drawing())
            .with_draw_to_masked_pixels(mask_bit_setting.draw_to_masked_pixels())
            // Other
            .with_interrupt_request(self.int_flag)
            .with_display_disabled(!self.display.enabled)
            // DMA related
            .with_dma_direction(self.dma_direction)
            .with_ready_to_receive_command(ready_to_receive_command)
            .with_ready_to_send_vram(ready_to_send_vram)
            .with_ready_to_receive_dma(ready_to_receive_dma)
            .with_dma_data_request(dma_data_request)
            // Some tests need this
            .with_drawing_even_odd_lines(if self.clock.vblank() {
                false
            } else {
                if self.display.interlaced {
                    self.draw_odd_even_frame
                } else {
                    self.clock.scanline & 1 != 0
                }
            })
    }

    pub fn update<'a>(
        &'a mut self,
        int_ctrl: &mut InterruptController,
        sys_cycles: u64,
    ) -> impl Iterator<Item = TimingSpan> + 'a {
        if self.int_flag {
            int_ctrl.raise(InterruptFlags::GPU);
        }

        self.clock.update(sys_cycles).inspect(|span| {
            if span.event.contains(TimingEvent::VBLANK_LEAVE) {
                self.draw_odd_even_frame = !self.draw_odd_even_frame;
            }
        })
    }
}

impl Mmio for Gpu {
    fn read(&mut self, dest: &mut [u8], maddr: u32) {
        match maddr {
            0x0..0x4 => {
                read_part::<4, 4>(dest, maddr, gp0::read(self).to_le_bytes());
            }
            0x4..0x8 => {
                read_part::<4, 4>(dest, maddr, self.stat().into_bytes());
            }
            _ => unimplemented!(),
        }
    }

    fn write(&mut self, maddr: u32, value: &[u8]) {
        match maddr {
            0x0..0x4 => {
                self.dispatch_gp0(u32::from_le_bytes(write_part::<4, 4>(maddr, value, [0; 4])));
            }
            0x4..0x8 => {
                self.dispatch_gp1(u32::from_le_bytes(write_part::<4, 4>(maddr, value, [0; 4])));
            }
            _ => unimplemented!(),
        }
    }
}
