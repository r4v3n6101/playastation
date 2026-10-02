use alloc::boxed::Box;
use core::mem;

use modular_bitfield::prelude::*;

use crate::{
    devices::{
        int::InterruptController,
        timer::{TimerController, TimingEvent},
    },
    render::{
        Renderer,
        noop::NoopRenderer,
        types::{RenderState, SemiTransparency, TextureDepth},
    },
    scheduler::{Event, Scheduler},
};

use super::{read_part, write_part};

mod clock;
mod gp0;
mod gp1;

const DEFAULT_HRANGE: (u16, u16) = (512, 3072);
const DEFAULT_VRANGE: (u16, u16) = (16, 256);

#[derive(Default, Clone, Copy)]
pub struct Display {
    pub hres: HorizontalResolution,
    pub vres: VerticalResolution,
    pub vmode: VideoMode,
    pub depth: DisplayDepth,
    pub interlaced: bool,
    pub special_368_hres: bool,
    pub reversed: bool,
    pub enabled: bool,
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
    pub display: Display,

    // Inner modules
    clock: clock::State,
    cmdbuf: gp0::CmdBuf,
    timing_dirty: bool,

    // GPU state itself
    frame_ready: bool,
    dma_direction: DmaDirection,

    int_flag: bool,
}

#[bitfield(bits = 32)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GpuStat {
    pub texture_page_x_base: B4,
    pub texture_page_y_base: B1,
    pub semi_transparency: SemiTransparency,
    pub texture_depth: TextureDepth,
    pub dither_24_to_15: bool,
    pub draw_to_display_area: bool,
    pub set_mask_while_drawing: bool,
    pub check_mask_before_drawing: bool,
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
            hrange: DEFAULT_HRANGE,
            vrange: DEFAULT_VRANGE,
            display: Display::default(),

            clock: clock::State::default(),
            cmdbuf: gp0::CmdBuf::default(),
            timing_dirty: true,

            frame_ready: false,
            dma_direction: DmaDirection::default(),

            int_flag: false,
        }
    }
}

impl Gpu {
    pub fn display_size(&self) -> (usize, usize) {
        let clocks_per_pixel = if self.display.special_368_hres {
            7
        } else {
            match self.display.hres {
                HorizontalResolution::H256 => 10,
                HorizontalResolution::H320 => 8,
                HorizontalResolution::H512 => 5,
                HorizontalResolution::H640 => 4,
            }
        };

        // here it is: https://psx-spx.consoledev.net/ps1/gpu/display-control-commands-gp1/#gp106h-horizontal-display-range-on-screen
        let (h0, h1) = self.hrange;
        let width = (usize::from(h1.saturating_sub(h0)) / clocks_per_pixel).wrapping_add(2) & !3;

        let (v0, v1) = self.vrange;
        let mut height = usize::from(v1.saturating_sub(v0));
        if self.display.interlaced && self.display.vres == VerticalResolution::V480 {
            height *= 2;
        }

        (width, height)
    }

    pub fn take_frame_ready(&mut self) -> bool {
        mem::take(&mut self.frame_ready)
    }

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
            .with_display_depth(self.display.depth)
            .with_interlace_field(!self.display.interlaced || self.clock.even_field())
            .with_vertical_interlace(self.display.interlaced)
            .with_special_hres_368(self.display.special_368_hres)
            .with_reverse_flag(self.display.reversed)
            // Via [`MaskBitSetting`]
            .with_set_mask_while_drawing(mask_bit_setting.set_mask_while_drawing())
            .with_check_mask_before_drawing(mask_bit_setting.check_mask_before_drawing())
            // Other
            .with_interrupt_request(self.int_flag)
            .with_display_disabled(!self.display.enabled)
            // DMA related
            .with_dma_direction(self.dma_direction)
            .with_ready_to_receive_command(ready_to_receive_command)
            .with_ready_to_send_vram(ready_to_send_vram)
            .with_ready_to_receive_dma(ready_to_receive_dma)
            .with_dma_data_request(dma_data_request)
            .with_drawing_even_odd_lines(if self.clock.vblank() {
                false
            } else if self.display.interlaced && self.display.vres == VerticalResolution::V480 {
                // i hate interlacing
                (self.vram_start.1 & 1 != 0) ^ !self.clock.even_field()
            } else {
                self.clock.scanline() & 1 != 0
            })
    }

    pub(crate) fn update(
        &mut self,
        scheduler: &mut Scheduler,
        int_ctrl: &mut InterruptController,
        timer_ctrl: &mut TimerController,
    ) {
        let elapsed = scheduler.take_elapsed(Event::Gpu);

        for span in self.clock.update(elapsed) {
            timer_ctrl.update(int_ctrl, span);
            if span.event.contains(TimingEvent::VBLANK_ENTER) {
                self.frame_ready = true;
            }
        }

        // Advance with the old timing before rebuilding deadlines for GP1 changes.
        if mem::take(&mut self.timing_dirty) {
            self.clock.set_display_mode(
                self.display.vmode,
                self.display.hres,
                self.display.special_368_hres,
                self.display.interlaced,
            );
            self.clock.set_display_ranges(self.hrange, self.vrange);
        }

        let mut delay = self.clock.delay_till_next_event();
        if let Some(timer_delay) =
            timer_ctrl.delay_till_next_event(self.clock.hblank(), self.clock.vblank(), |dots| {
                self.clock.delay_till_dotclocks(dots)
            })
        {
            delay = delay.min(timer_delay);
        }

        scheduler.remove(Event::Gpu);
        scheduler.schedule(Event::Gpu, delay);
    }

    pub(crate) fn read_mmio(&mut self, dest: &mut [u8], maddr: u32) {
        match maddr {
            0x0..0x4 => {
                read_part::<4, 4>(dest, maddr, self.gpuread().to_le_bytes());
            }
            0x4..0x8 => {
                read_part::<4, 4>(dest, maddr, self.stat().into_bytes());
            }
            _ => unimplemented!(),
        }
    }

    pub(crate) fn write_mmio(
        &mut self,
        scheduler: &mut Scheduler,
        int_ctrl: &mut InterruptController,
        maddr: u32,
        value: &[u8],
    ) {
        match maddr {
            0x0..0x4 => {
                self.dispatch_gp0(
                    int_ctrl,
                    u32::from_le_bytes(write_part::<4, 4>(maddr, value, [0; 4])),
                );
            }
            0x4..0x8 => {
                let was_dirty = self.timing_dirty;
                self.dispatch_gp1(u32::from_le_bytes(write_part::<4, 4>(maddr, value, [0; 4])));
                if !was_dirty && self.timing_dirty {
                    scheduler.schedule(Event::Gpu, 0);
                }
            }
            _ => unimplemented!(),
        }
    }
}
