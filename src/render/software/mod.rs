use std::{
    sync::mpsc::Sender,
    thread::{self, JoinHandle},
};

use triple_buffer::{Input, Output};

use super::{
    Renderer,
    types::{Polygon, Polyline, Position, Rect, Size},
};

mod backend;

pub struct SoftwareRenderer {
    __backend_thread: JoinHandle<()>,

    cmd_tx: Sender<backend::Command>,

    vram_view: Output<backend::Vram>,
    upload_buf: Input<backend::UploadBuf>,

    download_area: (Position, Size),
    upload_area: (Position, Size),
    pop_counter: u16,
}

impl Default for SoftwareRenderer {
    fn default() -> Self {
        let (worker, cmd_tx, vram_view, upload_buf) = backend::Worker::new();

        let __backend_thread = thread::Builder::new()
            .name("software-render-backend".to_string())
            .spawn(|| worker.run())
            .expect("backend thread start");

        Self {
            __backend_thread,

            cmd_tx,

            vram_view,
            upload_buf,

            download_area: (Position { x: 0, y: 0 }, Size { w: 0, h: 0 }),
            upload_area: (Position { x: 0, y: 0 }, Size { w: 0, h: 0 }),
            pop_counter: 0,
        }
    }
}

impl Renderer for SoftwareRenderer {
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

        let _ = self.cmd_tx.send(backend::Command::UploadVram {
            pos: self.upload_area.0,
            size: self.upload_area.1,
        });
    }

    fn reset(&mut self) {
        self.download_area = (Position { x: 0, y: 0 }, Size { w: 0, h: 0 });
        self.upload_area = (Position { x: 0, y: 0 }, Size { w: 0, h: 0 });
        self.pop_counter = 0;
        self.upload_buf.input_buffer_mut().clear();
    }
}
