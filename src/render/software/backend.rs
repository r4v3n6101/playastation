use std::{
    collections::VecDeque,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
};

use crossbeam_utils::atomic::AtomicCell;
use triple_buffer::{Input, Output, triple_buffer};

use super::super::types::{Location, Polygon, Polyline, Position, Rect, Size, Vertex};

pub const VRAM_WIDTH: usize = 1024;
pub const VRAM_HEIGHT: usize = 512;

pub type Vram = Box<[u16]>;
pub type UploadBuf = VecDeque<u16>;

pub struct SharedState {
    pub draw_area: (AtomicCell<Position>, AtomicCell<Position>),
    pub draw_offset: AtomicCell<Location>,
}

pub enum Command {
    DrawPolygon(Polygon),
    DrawPolyline(Polyline),
    DrawRect(Rect),
    SyncUploadBufToVram { pos: Position, size: Size },
}

pub struct Worker {
    cmd_rx: Receiver<Command>,

    vram: Vram,
    vram_view: Input<Vram>,
    upload_buf: Output<UploadBuf>,

    state: Arc<SharedState>,
}

impl Worker {
    pub fn new() -> (
        Sender<Command>,
        Output<Vram>,
        Input<UploadBuf>,
        Arc<SharedState>,
        Self,
    ) {
        let (cmd_tx, cmd_rx) = mpsc::channel();

        let vram = vec![0; VRAM_WIDTH * VRAM_HEIGHT].into_boxed_slice();
        let (vram_view, vram_out) = triple_buffer(&vram);
        let (upload_buf_in, upload_buf) = triple_buffer(&VecDeque::new());

        let state = Arc::new(SharedState {
            draw_area: (
                AtomicCell::new(Position { x: 0, y: 0 }),
                AtomicCell::new(Position {
                    x: VRAM_WIDTH as _,
                    y: VRAM_HEIGHT as _,
                }),
            ),
            draw_offset: AtomicCell::new(Location { x: 0, y: 0 }),
        });

        (
            cmd_tx,
            vram_out,
            upload_buf_in,
            state.clone(),
            Self {
                cmd_rx,
                vram,

                vram_view,
                upload_buf,

                state,
            },
        )
    }

    #[tracing::instrument(target = "render.software", level = "DEBUG", "run", skip(self))]
    pub fn run(mut self) {
        while let Ok(cmd) = self.cmd_rx.recv() {
            match cmd {
                Command::DrawRect(_) => {}
                Command::DrawPolygon(polygon) => match polygon.vertices.len() {
                    ..3 => tracing::warn!("degenerate polygon"),
                    3 => {
                        self.draw_triangle([
                            polygon.vertices[0],
                            polygon.vertices[1],
                            polygon.vertices[2],
                        ]);
                    }
                    4 => {
                        self.draw_triangle([
                            polygon.vertices[0],
                            polygon.vertices[1],
                            polygon.vertices[2],
                        ]);
                        self.draw_triangle([
                            polygon.vertices[1],
                            polygon.vertices[2],
                            polygon.vertices[3],
                        ]);
                    }
                    len => {
                        tracing::debug!(%len, "polygons larger than a quad aren't supported")
                    }
                },
                Command::SyncUploadBufToVram { pos, size } => {
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

        tracing::debug!("backend worker done");
    }

    #[tracing::instrument(
        target = "render.software",
        level = "DEBUG",
        "copy_vram_from_upload_buf",
        skip(self)
    )]
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

    #[tracing::instrument(
        target = "render.software",
        level = "DEBUG",
        "draw_triangle",
        skip(self)
    )]
    fn draw_triangle(
        &mut self,
        [
            Vertex {
                location: v0,
                color: c0,
                texcords: uv0,
            },
            Vertex {
                location: v1,
                color: c1,
                texcords: uv1,
            },
            Vertex {
                location: v2,
                color: c2,
                texcords: uv2,
            },
        ]: [Vertex; 3],
    ) {
        let draw_area = (self.state.draw_area.0.load(), self.state.draw_area.1.load());
        let draw_offset = self.state.draw_offset.load();

        let v0 = Location {
            x: v0.x + draw_offset.x,
            y: v0.y + draw_offset.y,
        };
        let v1 = Location {
            x: v1.x + draw_offset.x,
            y: v1.y + draw_offset.y,
        };
        let v2 = Location {
            x: v2.x + draw_offset.x,
            y: v2.y + draw_offset.y,
        };

        // bounding box
        let min_x =
            v0.x.min(v1.x)
                .min(v2.x)
                .clamp(draw_area.0.x as _, draw_area.1.x as _);
        let max_x =
            v0.x.max(v1.x)
                .max(v2.x)
                .clamp(draw_area.0.x as _, draw_area.1.x as _);
        let min_y =
            v0.y.min(v1.y)
                .min(v2.y)
                .clamp(draw_area.0.y as _, draw_area.1.y as _);
        let max_y =
            v0.y.max(v1.y)
                .max(v2.y)
                .clamp(draw_area.0.y as _, draw_area.1.y as _);

        // total signed area
        let area = cross2(v0, v1, v2);
        if area == 0 {
            return;
        }

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let p = Location { x, y };

                // barycentric weights
                let w0 = cross2(v1, v2, p);
                let w1 = cross2(v2, v0, p);
                let w2 = cross2(v0, v1, p);

                // inside test, both counter and clockwise (no backface culling)
                let inside = (w0 >= 0 && w1 >= 0 && w2 >= 0) || (w0 <= 0 && w1 <= 0 && w2 <= 0);
                if !inside {
                    continue;
                }

                // interpolate color
                // let r = (w0 * c0.color.r as i32 + w1 * v1.color.r as i32 + w2 * v2.color.r as i32)
                //     / area;
                // let g = (w0 * v0.color.g as i32 + w1 * v1.color.g as i32 + w2 * v2.color.g as i32)
                //     / area;
                // let b = (w0 * v0.color.b as i32 + w1 * v1.color.b as i32 + w2 * v2.color.b as i32)
                //     / area;
                //
                // let color = rgb888_to_bgr555(
                //     r.clamp(0, 255) as u8,
                //     g.clamp(0, 255) as u8,
                //     b.clamp(0, 255) as u8,
                // );
                //
                let idx = y as usize * VRAM_WIDTH + x as usize;

                self.vram[idx] = 0xFFFF;
            }
        }
    }
}

fn cross2(a: Location, b: Location, p: Location) -> i16 {
    (p.x - a.x) * (b.y - a.y) - (p.y - a.y) * (b.x - a.x)
}
