use alloc::{boxed::Box, collections::VecDeque};
use core::mem;

use super::{
    Renderer,
    types::{
        Color, DrawMode, EnvParameter, Location, MaskBitSetting, Polygon, Polyline, Position, Rect,
        RenderState, Size, TextureDepth, TextureWindow, UV, VRAM_HEIGHT, VRAM_WIDTH, Vertex, Vram,
    },
};

type ScreenFillCallback = Box<dyn FnMut(&[u16], usize, usize) + Send>;

pub struct SoftwareRenderer {
    vram: Vram,
    draw_mode: DrawMode,
    texture_window: TextureWindow,
    draw_area: (Position, Position),
    draw_offset: Location,
    mask_bit_setting: MaskBitSetting,

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

            draw_mode: DrawMode::new(),
            texture_window: TextureWindow::new(),
            mask_bit_setting: MaskBitSetting::new(),
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
        RenderState {
            draw_mode: self.draw_mode,
            mask_bit_setting: self.mask_bit_setting,
            vram_read_active: u32::from(self.pop_counter)
                < (u32::from(self.download_area.1.w) * u32::from(self.download_area.1.h)),
        }
    }

    fn draw_frame(&mut self) {
        (self.screen_fill)(&self.vram, VRAM_WIDTH, VRAM_HEIGHT);
    }

    fn set_parameter(&mut self, param: EnvParameter) {
        match param {
            EnvParameter::DrawMode(draw_mode) => {
                self.draw_mode = draw_mode;
            }
            EnvParameter::TextureWindow(texture_window) => {
                self.texture_window = texture_window;
            }
            EnvParameter::DrawAreaTopLeft(position) => {
                self.draw_area.0 = position;
            }
            EnvParameter::DrawAreaBottomRight(position) => {
                self.draw_area.1 = position;
            }
            EnvParameter::DrawOffset(location) => {
                self.draw_offset = location;
            }
            EnvParameter::MaskBitSetting(mask_bit_setting) => {
                self.mask_bit_setting = mask_bit_setting;
            }
        }
    }

    fn draw_polygon(&mut self, polygon: Polygon) {
        if let Some(tex_page) = polygon.tpage {
            self.draw_mode.set_tex_page(tex_page);
        }

        let flat_color = polygon.flat_color.is_some();
        let textured = polygon.clut.is_some();
        let raw_texture = polygon.raw_texture;
        let mut rasterize = |a, b, c| match (flat_color, textured, raw_texture) {
            (true, false, false) => self.rasterize_triangle::<true, false, false>(a, b, c),
            (false, false, false) => self.rasterize_triangle::<false, false, false>(a, b, c),

            (true, true, false) => self.rasterize_triangle::<true, true, false>(a, b, c),
            (false, true, false) => self.rasterize_triangle::<false, true, false>(a, b, c),

            (true, _, true) => self.rasterize_triangle::<true, true, true>(a, b, c),
            (false, _, true) => self.rasterize_triangle::<false, true, true>(a, b, c),
        };
        match polygon.vertices.len() {
            3 => {
                rasterize(
                    polygon.flat_color,
                    polygon.clut,
                    [
                        polygon.vertices[0],
                        polygon.vertices[1],
                        polygon.vertices[2],
                    ],
                );
            }
            4 => {
                rasterize(
                    polygon.flat_color,
                    polygon.clut,
                    [
                        polygon.vertices[0],
                        polygon.vertices[1],
                        polygon.vertices[2],
                    ],
                );
                rasterize(
                    polygon.flat_color,
                    polygon.clut,
                    [
                        polygon.vertices[1],
                        polygon.vertices[2],
                        polygon.vertices[3],
                    ],
                );
            }
            _ => {}
        }
    }

    fn draw_polyline(&mut self, _: Polyline) {}

    fn draw_rect(&mut self, rect: Rect) {
        let textured = rect.texcoords.is_some();
        let raw_texture = rect.raw_texture;

        match (textured, raw_texture) {
            (false, _) => self.rasterize_rect::<false, false>(rect),
            (true, false) => self.rasterize_rect::<true, false>(rect),
            (_, true) => self.rasterize_rect::<true, true>(rect),
        }
    }

    fn fill_vram_area(
        &mut self,
        Position { x, y }: Position,
        Size { w, h }: Size,
        Color { r, g, b }: Color,
    ) {
        let w = w as usize;
        let h = h as usize;
        for j in 0..h {
            for i in 0..w {
                let (x, y) = (x + i, y + j);
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
            let x = (x + i % w).clamp(0, VRAM_WIDTH - 1);
            let y = (y + i / w).clamp(0, VRAM_HEIGHT - 1);

            self.pop_counter += 1;
            return Some(self.vram[y * VRAM_WIDTH + x]);
        }

        None
    }

    fn prepare_vram_for_write(&mut self, pos: Position, size: Size) {
        self.upload_area = (pos, size);
        self.push_counter = 0;
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
            let x = (x + i % w).clamp(0, VRAM_WIDTH - 1);
            let y = (y + i / w).clamp(0, VRAM_HEIGHT - 1);

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
        let w = w as usize;
        let h = h as usize;

        // areas may overlap
        let mut tmp = VecDeque::with_capacity(w * h);

        for y in 0..h {
            for x in 0..w {
                let (x, y) = (sx + x, sy + y);
                if (0..VRAM_WIDTH).contains(&x) && (0..VRAM_HEIGHT).contains(&y) {
                    tmp.push_front(self.vram[y * VRAM_WIDTH + x]);
                }
            }
        }

        for y in 0..h {
            for x in 0..w {
                let (x, y) = (dx + x, dy + y);
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
    /// SAFETY: all unsafe are contracted by const-generics
    #[inline(never)]
    fn rasterize_triangle<const FLAT_COLOR: bool, const TEXTURED: bool, const RAW_TEXTURE: bool>(
        &mut self,
        flat_color: Option<Color>,
        clut: Option<Position>,
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

                // interpolate color
                let (r, g, b) = if FLAT_COLOR {
                    let Color { r, g, b } = unsafe { flat_color.unwrap_unchecked() };

                    (r, g, b)
                } else {
                    let (
                        Color {
                            r: r0,
                            g: g0,
                            b: b0,
                        },
                        Color {
                            r: r1,
                            g: g1,
                            b: b1,
                        },
                        Color {
                            r: r2,
                            g: g2,
                            b: b2,
                        },
                    ) = unsafe {
                        (
                            c0.unwrap_unchecked(),
                            c1.unwrap_unchecked(),
                            c2.unwrap_unchecked(),
                        )
                    };
                    (
                        ((w0 * r0 as i32 + w1 * r1 as i32 + w2 * r2 as i32) / area) as u8,
                        ((w0 * g0 as i32 + w1 * g1 as i32 + w2 * g2 as i32) / area) as u8,
                        ((w0 * b0 as i32 + w1 * b1 as i32 + w2 * b2 as i32) / area) as u8,
                    )
                };

                let color = if TEXTURED {
                    let (UV { u: u0, v: v0 }, UV { u: u1, v: v1 }, UV { u: u2, v: v2 }) = unsafe {
                        (
                            uv0.unwrap_unchecked(),
                            uv1.unwrap_unchecked(),
                            uv2.unwrap_unchecked(),
                        )
                    };
                    // interpolate texcoords
                    let (u, v) = (
                        ((w0 * u0 as i32 + w1 * u1 as i32 + w2 * u2 as i32) / area) as u8,
                        ((w0 * v0 as i32 + w1 * v1 as i32 + w2 * v2 as i32) / area) as u8,
                    );

                    let Some(texel) = self.sample_texture(clut, u, v) else {
                        continue;
                    };

                    if RAW_TEXTURE {
                        texel
                    } else {
                        modulate_bgr555(texel, r, g, b)
                    }
                } else {
                    rgb888_to_bgr555(r, g, b)
                };

                unsafe {
                    *self
                        .vram
                        .get_unchecked_mut(y as usize * VRAM_WIDTH + x as usize) = color;
                }
            }
        }
    }

    #[inline(never)]
    fn rasterize_rect<const TEXTURED: bool, const RAW_TEXTURE: bool>(&mut self, rect: Rect) {
        let draw_area = (self.draw_area.0, self.draw_area.1);
        let draw_offset = self.draw_offset;

        let pos = Location {
            x: rect.location.x + draw_offset.x,
            y: rect.location.y + draw_offset.y,
        };

        let Color { r, g, b } = rect.flat_color;
        for j in 0..rect.size.h {
            for i in 0..rect.size.w {
                let x = pos.x + i as i32;
                let y = pos.y + j as i32;

                if x < draw_area.0.x as i32
                    || x > draw_area.1.x as i32
                    || y < draw_area.0.y as i32
                    || y > draw_area.1.y as i32
                {
                    continue;
                }

                let (x, y) = (x as usize, y as usize);
                if x < VRAM_WIDTH && y < VRAM_HEIGHT {
                    let color = if TEXTURED {
                        let uv = unsafe { rect.texcoords.unwrap_unchecked() };
                        let u = if self.draw_mode.texture_rectangle_x_flip() {
                            uv.u.wrapping_sub(i as u8)
                        } else {
                            uv.u.wrapping_add(i as u8)
                        };

                        let v = if self.draw_mode.texture_rectangle_y_flip() {
                            uv.v.wrapping_sub(j as u8)
                        } else {
                            uv.v.wrapping_add(j as u8)
                        };

                        let Some(texel) = self.sample_texture(rect.clut, u, v) else {
                            continue;
                        };

                        if RAW_TEXTURE {
                            texel
                        } else {
                            modulate_bgr555(texel, r, g, b)
                        }
                    } else {
                        rgb888_to_bgr555(r, g, b)
                    };

                    unsafe {
                        *self.vram.get_unchecked_mut(y * VRAM_WIDTH + x) = color;
                    }
                }
            }
        }
    }

    #[inline(always)]
    fn sample_texture(&self, clut: Option<Position>, u: u8, v: u8) -> Option<u16> {
        let (base_x, base_y) = (
            self.draw_mode.tex_page().texture_page_x_base() as usize * 64,
            self.draw_mode.tex_page().texture_page_y_base() as usize * 256,
        );
        let (u, v) = self.apply_texture_window(u, v);
        let color = match (self.draw_mode.tex_page().texture_depth(), clut) {
            (TextureDepth::Bpp4, Some(clut)) => {
                self.fetch_clut_color(clut, self.fetch_index::<4>(base_x, base_y, u, v))
            }
            (TextureDepth::Bpp8, Some(clut)) => {
                self.fetch_clut_color(clut, self.fetch_index::<8>(base_x, base_y, u, v))
            }
            (TextureDepth::Bpp15, _) => self.fetch_15bpp(base_x, base_y, u, v),
            _ => {
                return None;
            }
        };

        // color=0 means transparent for textured rendering.
        if color.trailing_zeros() >= 15 {
            None
        } else {
            Some(color)
        }
    }

    #[inline(always)]
    fn apply_texture_window(&self, u: u8, v: u8) -> (u8, u8) {
        fn apply_texture_window_coord(coord: u8, mask: u8, offset: u8) -> u8 {
            (coord & !(mask << 3)) | ((offset & mask) << 3)
        }
        (
            apply_texture_window_coord(
                u,
                self.texture_window.mask_x(),
                self.texture_window.offset_x(),
            ),
            apply_texture_window_coord(
                v,
                self.texture_window.mask_y(),
                self.texture_window.offset_y(),
            ),
        )
    }

    #[inline(always)]
    fn fetch_index<const BPP: usize>(&self, base_x: usize, base_y: usize, u: u8, v: u8) -> u8 {
        let texels_per_pixel = 16 / BPP;
        let x = (base_x + (u as usize / texels_per_pixel)).clamp(0, VRAM_WIDTH - 1);
        let y = (base_y + v as usize).clamp(0, VRAM_HEIGHT - 1);

        let word = unsafe { *self.vram.get_unchecked(y * VRAM_WIDTH + x) };

        let shift = (u as usize % texels_per_pixel) * BPP;
        ((word >> shift) & ((1 << BPP) - 1)) as u8
    }

    #[inline(always)]
    fn fetch_15bpp(&self, base_x: usize, base_y: usize, u: u8, v: u8) -> u16 {
        let x = (base_x + u as usize).clamp(0, VRAM_WIDTH - 1);
        let y = (base_y + v as usize).clamp(0, VRAM_HEIGHT - 1);

        unsafe { *self.vram.get_unchecked(y * VRAM_WIDTH + x) }
    }

    #[inline(always)]
    fn fetch_clut_color(&self, clut: Position, index: u8) -> u16 {
        let x = (clut.x + index as usize).clamp(0, VRAM_WIDTH - 1);
        let y = clut.y.clamp(0, VRAM_HEIGHT - 1);

        unsafe { *self.vram.get_unchecked(y * VRAM_WIDTH + x) }
    }
}

fn cross2(a: Location, b: Location, p: Location) -> i32 {
    (p.x - a.x) * (b.y - a.y) - (p.y - a.y) * (b.x - a.x)
}

fn rgb888_to_bgr555(r: u8, g: u8, b: u8) -> u16 {
    let r5 = (r >> 3) as u16;
    let g5 = (g >> 3) as u16;
    let b5 = (b >> 3) as u16;

    r5 | (g5 << 5) | (b5 << 10)
}

fn modulate_bgr555(texel: u16, r: u8, g: u8, b: u8) -> u16 {
    let tr = (texel & 0x1f) as u32;
    let tg = ((texel >> 5) & 0x1f) as u32;
    let tb = ((texel >> 10) & 0x1f) as u32;
    let mask = texel & 0x8000;

    let r = ((tr * r as u32) >> 7).min(31) as u16;
    let g = ((tg * g as u32) >> 7).min(31) as u16;
    let b = ((tb * b as u32) >> 7).min(31) as u16;

    mask | r | (g << 5) | (b << 10)
}
