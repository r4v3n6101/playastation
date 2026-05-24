use core::mem;

use super::{
    Renderer,
    types::{
        Color, DrawMode, EnvParameter, MaskBitSetting, Polygon, Polyline, Position, Rect,
        RenderState, Size,
    },
};

#[derive(Debug)]
pub struct NoopRenderer {
    download_area: (Position, Size),
    upload_area: (Position, Size),
    pop_counter: u16,
    push_counter: u16,
}

impl Default for NoopRenderer {
    fn default() -> Self {
        Self {
            download_area: (Position { x: 0, y: 0 }, Size { w: 0, h: 0 }),
            upload_area: (Position { x: 0, y: 0 }, Size { w: 0, h: 0 }),
            pop_counter: 0,
            push_counter: 0,
        }
    }
}

impl Renderer for NoopRenderer {
    fn state(&self) -> RenderState {
        RenderState {
            draw_mode: DrawMode::new(),
            mask_bit_setting: MaskBitSetting::new(),
            vram_read_active: u32::from(self.pop_counter)
                < (u32::from(self.download_area.1.w) * u32::from(self.download_area.1.h)),
        }
    }

    fn draw_frame(&mut self) {
        tracing::debug!("goo-goo-guh-guh");
    }

    fn set_parameter(&mut self, param: EnvParameter) {
        tracing::debug!(?param, "change parameter");
    }

    fn draw_polygon(&mut self, polygon: Polygon) {
        tracing::debug!(?polygon, "draw polygon");
    }

    fn draw_polyline(&mut self, polyline: Polyline) {
        tracing::debug!(?polyline, "draw polyline");
    }

    fn draw_rect(&mut self, rect: Rect) {
        tracing::debug!(?rect, "draw rect");
    }

    fn fill_vram_area(&mut self, pos: Position, size: Size, color: Color) {
        tracing::debug!(?pos, ?size, ?color, "fill vram area");
    }

    fn prepare_vram_for_read(&mut self, pos: Position, size: Size) {
        self.download_area = (pos, size);
        self.pop_counter = 0;
        tracing::debug!(download_area=?self.download_area, "prepare vram for popping pixels");
    }

    fn pop_pixel(&mut self) -> Option<u16> {
        let size = u32::from(self.download_area.1.w) * u32::from(self.download_area.1.h);
        if u32::from(self.pop_counter) < size {
            self.pop_counter = self.pop_counter.saturating_add(1);
            tracing::trace!("pop pixel {}/{}", self.pop_counter, size);
        }

        Some(0)
    }

    fn prepare_vram_for_write(&mut self, pos: Position, size: Size) {
        self.upload_area = (pos, size);
        self.push_counter = 0;
        tracing::debug!(upload_area=?self.upload_area, "prepare vram for pushing pixels");
    }

    fn push_pixel(&mut self, pixel: u16) {
        let size = self.upload_area.1.w * self.upload_area.1.h;
        if self.push_counter < size {
            self.push_counter = self.push_counter.saturating_add(1);
            tracing::trace!(
                pixel=%format_args!("{pixel:#X}"),
                "push pixel {}/{}",
                self.push_counter,
                size
            );
        }
    }

    fn mirror_vram_area(&mut self, src: Position, dest: Position, size: Size) {
        tracing::debug!(?src, ?dest, ?size, "mirror vram area");
    }

    fn reset(&mut self) {
        mem::take(self);
    }
}
