use alloc::{boxed::Box, collections::VecDeque};
use core::mem;

use super::{
    Renderer,
    types::{
        Color, Location, Polygon, Polyline, Position, Rect, RenderState, Size, TextureWindow,
        VRAM_HEIGHT, VRAM_WIDTH, Vertex, Vram,
    },
};

type ScreenFillCallback = Box<dyn FnMut(&[u16], usize, usize) + Send>;

pub struct SoftwareRenderer {
    vram: Vram,
    texture_window: TextureWindow,
    draw_area: (Position, Position),
    draw_offset: Location,

    download_area: (Position, Size),
    upload_area: (Position, Size),
    pop_counter: u16,
    push_counter: u16,

    pub screen_fill: ScreenFillCallback,
}

impl Default for SoftwareRenderer {
    fn default() -> Self {
        let vram = alloc::vec![0; VRAM_WIDTH * VRAM_HEIGHT].into_boxed_slice();
        Self {
            vram,

            texture_window: TextureWindow {
                mask_x: 0,
                mask_y: 0,
                offset_x: 0,
                offset_y: 0,
            },
            draw_area: (Position { x: 0, y: 0 }, Position { x: 0, y: 0 }),
            draw_offset: Location { x: 0, y: 0 },

            download_area: (Position { x: 0, y: 0 }, Size { w: 0, h: 0 }),
            upload_area: (Position { x: 0, y: 0 }, Size { w: 0, h: 0 }),
            pop_counter: 0,
            push_counter: 0,

            screen_fill: Box::new(|_, _, _| {}),
        }
    }
}

impl Renderer for SoftwareRenderer {
    fn state(&self) -> RenderState {
        RenderState {}
    }

    fn draw_frame(&mut self) {
        (self.screen_fill)(&self.vram, VRAM_WIDTH, VRAM_HEIGHT);
    }

    fn set_texture_window(&mut self, tex_win: TextureWindow) {
        self.texture_window = tex_win;
    }

    fn set_draw_area_top_left(&mut self, pos: Position) {
        self.draw_area.0 = pos;
    }

    fn set_draw_area_bottom_right(&mut self, pos: Position) {
        self.draw_area.1 = pos;
    }

    fn set_draw_offset(&mut self, loc: Location) {
        self.draw_offset = loc;
    }

    fn draw_polygon(&mut self, polygon: Polygon) {
        match polygon.vertices.len() {
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
        }
    }

    fn draw_polyline(&mut self, _: Polyline) {}

    fn draw_rect(&mut self, rect: Rect) {
        self.draw_rect(rect);
    }

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

    fn prepare_vram_for_read(&mut self, pos: Position, size: Size) {
        self.download_area = (pos, size);
        self.pop_counter = 0;
    }

    fn pop_pixel(&mut self) -> Option<u16> {
        let (Position { x, y }, Size { w, h }) = self.download_area;
        if w == 0 || h == 0 {
            return None;
        }

        let w = w as usize;
        let h = h as usize;
        let i = self.pop_counter as usize;
        if i < w * h {
            let x = (x as usize + i % w).clamp(0, VRAM_WIDTH - 1);
            let y = (y as usize + i / w).clamp(0, VRAM_HEIGHT - 1);

            self.pop_counter += 1;
            return Some(self.vram[y * VRAM_WIDTH + x]);
        }

        None
    }

    fn prepare_vram_for_write(&mut self, pos: Position, size: Size) {
        self.upload_area = (pos, size);
    }

    fn push_pixel(&mut self, pixel: u16) {
        let (Position { x, y }, Size { w, h }) = self.upload_area;

        if w == 0 || h == 0 {
            return;
        }

        let w = w as usize;
        let h = h as usize;
        let i = self.push_counter as usize;
        if i < w * h {
            let x = (x as usize + (i % w)).clamp(0, VRAM_WIDTH - 1);
            let y = (y as usize + (i / w)).clamp(0, VRAM_HEIGHT - 1);

            self.vram[y * VRAM_WIDTH + x] = pixel;
            self.push_counter += 1;
        }
    }

    fn mirror_vram_area(
        &mut self,
        Position { x: sx, y: sy }: Position,
        Position { x: dx, y: dy }: Position,
        Size { w, h }: Size,
    ) {
        // areas may overlap
        let mut tmp = VecDeque::with_capacity(w as usize * h as usize);

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

    fn reset(&mut self) {
        let mut prev = mem::take(self);
        mem::swap(&mut prev.screen_fill, &mut self.screen_fill);
    }
}

impl SoftwareRenderer {
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
        let draw_area = (self.draw_area.0, self.draw_area.1);
        let draw_offset = self.draw_offset;

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

    fn draw_rect(&mut self, rect: Rect) {
        let draw_area = (self.draw_area.0, self.draw_area.1);
        let draw_offset = self.draw_offset;

        let pos = Location {
            x: rect.location.x + draw_offset.x,
            y: rect.location.y + draw_offset.y,
        };

        for j in 0..rect.size.h {
            for i in 0..rect.size.w {
                let x = pos.x + i as i16;
                let y = pos.y + j as i16;

                if x < draw_area.0.x as i16
                    || x > draw_area.1.x as i16
                    || y < draw_area.0.y as i16
                    || y > draw_area.1.y as i16
                {
                    continue;
                }

                let x = x as usize;
                let y = y as usize;
                if x < VRAM_WIDTH || y < VRAM_HEIGHT {
                    self.vram[y * VRAM_WIDTH + x] =
                        rgb888_to_bgr555(rect.flat_color.r, rect.flat_color.g, rect.flat_color.b);
                }
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
