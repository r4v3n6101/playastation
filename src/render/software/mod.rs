use std::{
    mem,
    sync::{Arc, mpsc::Sender},
    thread::{self, JoinHandle},
};

use triple_buffer::{Input, Output};

use super::{
    Renderer,
    types::{Location, Polygon, Polyline, Position, Rect, RenderState, Size},
};

mod backend;

pub struct SoftwareRenderer {
    /// Command channel.
    cmd_tx: Sender<backend::Command>,
    /// Vram back-channel (i.e. receiver).
    vram_view: Output<backend::Vram>,
    /// Upload buf channel (as a sender).
    upload_buf: Input<backend::UploadBuf>,

    state: Arc<backend::SharedState>,
    download_area: (Position, Size),
    upload_area: (Position, Size),
    pop_counter: u16,

    __backend_thread: JoinHandle<()>,
}

impl Default for SoftwareRenderer {
    fn default() -> Self {
        let (cmd_tx, vram_view, upload_buf, state, worker) = backend::Worker::new();

        Self {
            cmd_tx,
            vram_view,
            upload_buf,

            state,
            download_area: (Position { x: 0, y: 0 }, Size { w: 0, h: 0 }),
            upload_area: (Position { x: 0, y: 0 }, Size { w: 0, h: 0 }),
            pop_counter: 0,

            __backend_thread: thread::Builder::new()
                .name("software-render-backend".to_string())
                .spawn(|| worker.run())
                .expect("backend thread start"),
        }
    }
}

impl Renderer for SoftwareRenderer {
    fn state(&self) -> RenderState {
        RenderState {
            draw_area: (
                Position {
                    x: self.state.draw_area.0.load().x,
                    y: self.state.draw_area.0.load().y,
                },
                Position {
                    x: self.state.draw_area.1.load().x,
                    y: self.state.draw_area.1.load().y,
                },
            ),
            draw_offset: Location {
                x: self.state.draw_offset.load().x,
                y: self.state.draw_offset.load().y,
            },
        }
    }

    fn set_draw_area_top_left(&mut self, pos: Position) {
        self.state.draw_area.0.store(pos);
    }

    fn set_draw_area_bottom_right(&mut self, pos: Position) {
        self.state.draw_area.1.store(pos);
    }

    fn set_draw_offset(&mut self, loc: Location) {
        self.state.draw_offset.store(loc);
    }

    fn draw_polygon(&mut self, polygon: Polygon) {
        let _ = self.cmd_tx.send(backend::Command::DrawPolygon(polygon));
    }

    fn draw_polyline(&mut self, polyline: Polyline) {
        let _ = self.cmd_tx.send(backend::Command::DrawPolyline(polyline));
    }

    fn draw_rect(&mut self, rect: Rect) {
        let _ = self.cmd_tx.send(backend::Command::DrawRect(rect));
    }

    fn download_vram_area_to_local(&mut self, pos: Position, size: Size) {
        self.download_area = (pos, size);
        self.pop_counter = 0;
        self.vram_view.update();
    }

    fn pop_pixel(&mut self) -> Option<u16> {
        let size = self.download_area.1.w * self.download_area.1.h;
        if self.pop_counter < size {
            let (Position { x, y }, Size { w, .. }) = self.download_area;
            let x = (x + self.pop_counter % w) as usize;
            let y = (y + self.pop_counter / w) as usize;

            self.pop_counter = self.pop_counter.saturating_add(1);
            if x < backend::VRAM_WIDTH && y < backend::VRAM_HEIGHT {
                let vram = self.vram_view.output_buffer();
                return Some(vram[y * backend::VRAM_WIDTH + x]);
            }
        }

        None
    }

    fn prepare_local_vram_to_upload(&mut self, pos: Position, size: Size) {
        self.upload_area = (pos, size);
        self.upload_buf.input_buffer_mut().clear();
    }

    fn push_pixel(&mut self, pixel: u16) {
        self.upload_buf.input_buffer_mut().push_back(pixel);
    }

    fn upload_local_vram_area(&mut self) {
        self.upload_buf.publish();

        let _ = self.cmd_tx.send(backend::Command::SyncUploadBufToVram {
            pos: self.upload_area.0,
            size: self.upload_area.1,
        });
    }

    fn reset(&mut self) {
        mem::take(self);
    }
}
