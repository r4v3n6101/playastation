use core::mem;

use super::{
    Renderer,
    types::{Color, Location, Polygon, Polyline, Position, Rect, RenderState, Size},
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
            draw_area: (Position { x: 0, y: 0 }, Position { x: 0, y: 0 }),
            draw_offset: Location { x: 0, y: 0 },
            vblank_int: false,
        }
    }

    fn set_draw_area_top_left(&mut self, pos: Position) {
        tracing::debug!(?pos, "draw area top left");
    }

    fn set_draw_area_bottom_right(&mut self, pos: Position) {
        tracing::debug!(?pos, "draw area bottom right");
    }

    fn set_draw_offset(&mut self, loc: Location) {
        tracing::debug!(?loc, "draw offset");
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

    fn download_vram_area_to_local(&mut self, pos: Position, size: Size) {
        self.download_area = (pos, size);
        self.pop_counter = 0;
        tracing::debug!(download_area=?self.download_area, "download vram area to local storage");
    }

    fn pop_pixel(&mut self) -> Option<u16> {
        let size = self.download_area.1.w * self.download_area.1.h;
        if self.pop_counter < size {
            self.pop_counter = self.pop_counter.saturating_add(1);
            tracing::trace!("pop pixel {}/{}", self.pop_counter, size);
        }

        Some(0)
    }

    fn prepare_local_vram_to_upload(&mut self, pos: Position, size: Size) {
        self.upload_area = (pos, size);
        self.push_counter = 0;
        tracing::debug!(upload_area=?self.upload_area, "prepare inner state for pixel filling");
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

    fn upload_local_vram_area(&mut self) {
        tracing::debug!(upload_area=?self.upload_area, "upload filled vram area");
    }

    fn mirror_vram_area(&mut self, src: Position, dest: Position, size: Size) {
        tracing::debug!(?src, ?dest, ?size, "mirror vram area");
    }

    fn clear_int(&mut self) {
        tracing::debug!("clear interruptions");
    }

    fn reset(&mut self) {
        mem::take(self);
    }
}
