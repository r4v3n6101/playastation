use std::{
    collections::VecDeque,
    sync::mpsc::{self, Receiver, Sender},
};

use triple_buffer::{Input, Output, triple_buffer};

use crate::render::types::{Polygon, Polyline, Position, Rect, Size};

pub const VRAM_WIDTH: usize = 1024;
pub const VRAM_HEIGHT: usize = 512;

pub type Vram = Box<[u16]>;
pub type UploadBuf = VecDeque<u16>;

pub enum Command {
    DrawPolygon(Polygon),
    DrawPolyline(Polyline),
    DrawRect(Rect),
    UploadVram { pos: Position, size: Size },
}

pub struct Worker {
    cmd_rx: Receiver<Command>,

    vram: Vram,
    vram_view: Input<Vram>,
    upload_buf: Output<UploadBuf>,
}

impl Worker {
    pub fn new() -> (Self, Sender<Command>, Output<Vram>, Input<UploadBuf>) {
        let (cmd_tx, cmd_rx) = mpsc::channel();

        let vram = vec![0; VRAM_WIDTH * VRAM_HEIGHT].into_boxed_slice();
        let (vram_view, vram_out) = triple_buffer(&vram);
        let (upload_buf_in, upload_buf) = triple_buffer(&VecDeque::new());

        (
            Self {
                cmd_rx,
                vram,

                vram_view,
                upload_buf,
            },
            cmd_tx,
            vram_out,
            upload_buf_in,
        )
    }

    pub fn run(mut self) {
        while let Ok(cmd) = self.cmd_rx.recv() {
            match cmd {
                Command::UploadVram { pos, size } => {
                    self.upload_buf.update();
                    self.copy_vram_from_upload_buf(pos, size);
                }
                _ => {}
            }

            self.vram_view
                .input_buffer_mut()
                .copy_from_slice(&self.vram);
            self.vram_view.publish();
        }
    }

    fn copy_vram_from_upload_buf(&mut self, Position { x, y }: Position, Size { w, h }: Size) {
        let data = self.upload_buf.output_buffer_mut();
        for j in 0..h {
            for i in 0..w {
                let (x, y) = (x + i, y + j);
                let idx = y as usize * VRAM_WIDTH + x as usize;
                if let Some(src) = self.vram.get_mut(idx) {
                    // Set black pixels if buf somehow smaller
                    *src = data.pop_front().unwrap_or_default();
                }
            }
        }
    }
}
