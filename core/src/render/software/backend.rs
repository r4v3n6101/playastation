use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::Duration,
};

use crossbeam_utils::atomic::AtomicCell;
use triple_buffer::{Input, Output, triple_buffer};

use super::super::types::{Color, Location, Polygon, Polyline, Position, Rect, Size, Vertex};

pub const VRAM_WIDTH: usize = 1024;
pub const VRAM_HEIGHT: usize = 512;

pub type Vram = Box<[u16]>;
pub type TextureBuf = VecDeque<u16>;
pub type ScreenFillCallback = Box<dyn FnMut(&[u16], usize, usize) + Send>;

pub struct SharedState {
    pub draw_area: (AtomicCell<Position>, AtomicCell<Position>),
    pub draw_offset: AtomicCell<Location>,
    pub vblank_int: AtomicBool,
    pub screen_fill: Mutex<ScreenFillCallback>,
}

pub enum Command {
    DrawPolygon(Polygon),
    DrawPolyline(Polyline),
    DrawRect(Rect),
    FillVramArea {
        pos: Position,
        size: Size,
        color: Color,
    },
    SyncUploadBufToVram {
        pos: Position,
        size: Size,
        data: TextureBuf,
    },
    MirrorVramArea {
        src: Position,
        dest: Position,
        size: Size,
    },
    SetDrawAreaTopLeft(Position),
    SetDrawAreaBottomRight(Position),
    SetDrawOffset(Location),
}

pub struct Worker {
    cmd_rx: Receiver<Command>,

    vram: Vram,
    vram_view: Input<Vram>,
    pub state: Arc<SharedState>,
}

impl Worker {
    pub fn new() -> (Sender<Command>, Output<Vram>, Self) {
        let (cmd_tx, cmd_rx) = mpsc::channel();

        let vram = vec![0; VRAM_WIDTH * VRAM_HEIGHT].into_boxed_slice();
        let (vram_view, vram_out) = triple_buffer(&vram);

        (
            cmd_tx,
            vram_out,
            Self {
                cmd_rx,

                vram,
                vram_view,
                state: Arc::new(SharedState {
                    draw_area: (
                        AtomicCell::new(Position { x: 0, y: 0 }),
                        AtomicCell::new(Position {
                            x: (VRAM_WIDTH - 1) as _,
                            y: (VRAM_HEIGHT - 1) as _,
                        }),
                    ),
                    draw_offset: AtomicCell::new(Location { x: 0, y: 0 }),
                    vblank_int: AtomicBool::new(true),

                    // FIXME : experimental
                    screen_fill: Mutex::new(Box::new(|_, _, _| {})),
                }),
            },
        )
    }

    #[tracing::instrument(target = "render.software", level = "DEBUG", "run", skip(self))]
    pub fn run(mut self) {
        while let Ok(cmd) = self.cmd_rx.recv() {
            match cmd {
                Command::DrawRect(_) => {}
                Command::DrawPolygon(polygon) => match polygon.vertices.len() {
                    len @ ..3 => tracing::debug!(%len, "degenerate polygon"),
                    3 => {
                        self.draw_triangle(
                            polygon.flat_color,
                            [
                                polygon.vertices[0],
                                polygon.vertices[1],
                                polygon.vertices[2],
                            ],
                        );
                    }
                    4 => {
                        self.draw_triangle(
                            polygon.flat_color,
                            [
                                polygon.vertices[0],
                                polygon.vertices[1],
                                polygon.vertices[2],
                            ],
                        );
                        self.draw_triangle(
                            polygon.flat_color,
                            [
                                polygon.vertices[1],
                                polygon.vertices[2],
                                polygon.vertices[3],
                            ],
                        );
                    }
                    len => {
                        tracing::debug!(%len, "polygons larger than a quad aren't supported");
                    }
                },
                Command::FillVramArea { pos, size, color } => {
                    self.fill_vram_area(pos, size, color);
                }
                Command::SyncUploadBufToVram { pos, size, data } => {
                    self.copy_vram_from_upload_buf(pos, size, data);
                }
                Command::MirrorVramArea { src, dest, size } => {
                    self.mirror_vram_area(src, dest, size);
                }
                Command::SetDrawAreaTopLeft(pos) => {
                    self.state.draw_area.0.store(pos);
                }
                Command::SetDrawAreaBottomRight(pos) => {
                    self.state.draw_area.1.store(pos);
                }
                Command::SetDrawOffset(loc) => {
                    self.state.draw_offset.store(loc);
                }
                _ => {}
            }

            self.vram_view
                .input_buffer_mut()
                .copy_from_slice(&self.vram);
            self.vram_view.publish();

            self.state.vblank_int.store(true, Ordering::Release);

            // FIXME: experimental, interface may be changed with high probability in the future
            (self.state.screen_fill.lock().unwrap())(&self.vram, VRAM_WIDTH, VRAM_HEIGHT);

            thread::sleep(Duration::from_secs(1) / 60);
        }

        tracing::debug!("backend worker done");
    }

    #[tracing::instrument(
        target = "render.software",
        level = "DEBUG",
        "fill_vram_area",
        skip(self)
    )]
    fn fill_vram_area(
        &mut self,
        Position { x, y }: Position,
        Size { w, h }: Size,
        Color { r, g, b }: Color,
    ) {
        for j in 0..h {
            for i in 0..w {
                let (x, y) = (x + i, y + j);
                let (x, y) = (x as usize, y as usize);
                if (0..VRAM_WIDTH).contains(&x) && (0..VRAM_HEIGHT).contains(&y) {
                    self.vram[y * VRAM_WIDTH + x] = rgb888_to_bgr555(r, g, b);
                }
            }
        }
    }

    #[tracing::instrument(
        target = "render.software",
        level = "DEBUG",
        "copy_vram_from_upload_buf",
        skip(self)
    )]
    fn copy_vram_from_upload_buf(
        &mut self,
        Position { x, y }: Position,
        Size { w, h }: Size,
        mut data: TextureBuf,
    ) {
        for j in 0..h {
            for i in 0..w {
                let (x, y) = (x + i, y + j);
                let (x, y) = (x as usize, y as usize);
                if (0..VRAM_WIDTH).contains(&x) && (0..VRAM_HEIGHT).contains(&y) {
                    self.vram[y * VRAM_WIDTH + x] = data.pop_front().unwrap();
                }
            }
        }
    }

    #[tracing::instrument(
        target = "render.software",
        level = "DEBUG",
        "mirror_vram_area",
        skip(self)
    )]
    fn mirror_vram_area(
        &mut self,
        Position { x: sx, y: sy }: Position,
        Position { x: dx, y: dy }: Position,
        Size { w, h }: Size,
    ) {
        // areas may overlap
        let mut tmp = TextureBuf::with_capacity(w as usize * h as usize);

        for y in 0..h {
            for x in 0..w {
                let (x, y) = (sx + x, sy + y);
                let (x, y) = (x as usize, y as usize);
                if (0..VRAM_WIDTH).contains(&x) && (0..VRAM_HEIGHT).contains(&y) {
                    tmp.push_front(self.vram[y * VRAM_WIDTH + x]);
                }
            }
        }

        for y in 0..h {
            for x in 0..w {
                let (x, y) = (dx + x, dy + y);
                let (x, y) = (x as usize, y as usize);
                if (0..VRAM_WIDTH).contains(&x) && (0..VRAM_HEIGHT).contains(&y) {
                    self.vram[y * VRAM_WIDTH + x] = tmp.pop_back().unwrap();
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
        flat_color: Option<Color>,
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

        // bounding box (clipped to reduce cycle loops)
        let min_x =
            v0.x.min(v1.x)
                .min(v2.x)
                .clamp(draw_area.0.x as _, draw_area.1.x as _)
                .clamp(0, (VRAM_WIDTH - 1) as _);
        let max_x =
            v0.x.max(v1.x)
                .max(v2.x)
                .clamp(draw_area.0.x as _, draw_area.1.x as _)
                .clamp(0, (VRAM_WIDTH - 1) as _);
        let min_y =
            v0.y.min(v1.y)
                .min(v2.y)
                .clamp(draw_area.0.y as _, draw_area.1.y as _)
                .clamp(0, (VRAM_HEIGHT - 1) as _);
        let max_y =
            v0.y.max(v1.y)
                .max(v2.y)
                .clamp(draw_area.0.y as _, draw_area.1.y as _)
                .clamp(0, (VRAM_HEIGHT - 1) as _);

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
                let inside = if area > 0 {
                    w0 >= 0 && w1 >= 0 && w2 >= 0
                } else {
                    w0 <= 0 && w1 <= 0 && w2 <= 0
                };
                if !inside {
                    continue;
                }

                debug_assert_eq!(w0 + w1 + w2, area);

                let c0 = c0.or(flat_color).unwrap_or(Color { r: 0, g: 0, b: 0 });
                let c1 = c1.or(flat_color).unwrap_or(Color { r: 0, g: 0, b: 0 });
                let c2 = c2.or(flat_color).unwrap_or(Color { r: 0, g: 0, b: 0 });

                // interpolate color
                let r = (w0 * c0.r as i32 + w1 * c1.r as i32 + w2 * c2.r as i32) / area;
                let g = (w0 * c0.g as i32 + w1 * c1.g as i32 + w2 * c2.g as i32) / area;
                let b = (w0 * c0.b as i32 + w1 * c1.b as i32 + w2 * c2.b as i32) / area;

                // interpolate texcoords
                // let uv = if let Some(uv0) = uv0
                //     && let Some(uv1) = uv1
                //     && let Some(uv2) = uv2
                // {
                //     Some((
                //         (w0 * uv0.u as i32 + w1 * uv1.u as i32 + w2 * uv2.u as i32) / area,
                //         (w0 * uv0.v as i32 + w1 * uv1.v as i32 + w2 * uv2.v as i32) / area,
                //     ))
                // } else {
                //     None
                // };

                let color = rgb888_to_bgr555(
                    r.clamp(0, 255) as u8,
                    g.clamp(0, 255) as u8,
                    b.clamp(0, 255) as u8,
                );

                let idx = y as usize * VRAM_WIDTH + x as usize;
                self.vram[idx] = color;
            }
        }
    }
}

fn cross2(a: Location, b: Location, p: Location) -> i32 {
    (p.x as i32 - a.x as i32) * (b.y as i32 - a.y as i32)
        - (p.y as i32 - a.y as i32) * (b.x as i32 - a.x as i32)
}

fn rgb888_to_bgr555(r: u8, g: u8, b: u8) -> u16 {
    let r5 = (r >> 3) as u16;
    let g5 = (g >> 3) as u16;
    let b5 = (b >> 3) as u16;

    r5 | (g5 << 5) | (b5 << 10)
}
