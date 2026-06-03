use alloc::{boxed::Box, collections::VecDeque};
use core::mem;

use fixed::{FixedI64, types::extra::U32};

use super::{
    Renderer,
    types::{
        Color, DrawMode, EnvParameter, Location, MaskBitSetting, Polygon, Polyline, Position, Rect,
        RenderState, Size, TextureDepth, TextureWindow, VRAM_HEIGHT, VRAM_WIDTH, Vertex, Vram,
    },
};

/// Fixed point calculation for rasterizator.
type FP = FixedI64<U32>;
// NB: experimental
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

    screen_fill: ScreenFillCallback,
}

impl Default for SoftwareRenderer {
    fn default() -> Self {
        Self {
            vram: alloc::vec![0; VRAM_WIDTH * VRAM_HEIGHT].into_boxed_slice(),

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

impl SoftwareRenderer {
    pub fn with_screen_fill(screen_fill: ScreenFillCallback) -> Self {
        Self {
            screen_fill,
            ..Default::default()
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
        let raw_texture = polygon.raw_texture;
        // 0 means no texture at all
        let tex_depth = if let Some(tpage) = polygon.tpage {
            match tpage.texture_depth() {
                TextureDepth::Bpp15 => 15,
                TextureDepth::Bpp8 => 8,
                TextureDepth::Bpp4 => 4,
                _ => 0,
            }
        } else {
            0
        };
        let mut rasterize = |a, b, c| match (flat_color, tex_depth, raw_texture) {
            // 15BPP, textured, flat
            (true, 15, false) => self.rasterize_triangle::<true, 15, false>(a, b, c),
            // 15BPP, textured, Gouraud
            (false, 15, false) => self.rasterize_triangle::<false, 15, false>(a, b, c),

            // 8BPP, textured, flat
            (true, 8, false) => self.rasterize_triangle::<true, 8, false>(a, b, c),
            // 8BPP, textured, Gouraud
            (false, 8, false) => self.rasterize_triangle::<false, 8, false>(a, b, c),

            // 4BPP, textured, flat
            (true, 4, false) => self.rasterize_triangle::<true, 4, false>(a, b, c),
            // 4BPP, textured, Gouraud
            (false, 4, false) => self.rasterize_triangle::<false, 4, false>(a, b, c),

            // 15BPP, textured, raw texture
            (_, 15, true) => self.rasterize_triangle::<false, 15, true>(a, b, c),
            // 8BPP, textured, raw texture
            (_, 8, true) => self.rasterize_triangle::<false, 8, true>(a, b, c),
            // 4BPP, textured, raw texture
            (_, 4, true) => self.rasterize_triangle::<false, 4, true>(a, b, c),

            // untextured, flat
            (true, _, _) => self.rasterize_triangle::<true, 0, false>(a, b, c),
            // untextured, Gouraud
            (false, _, _) => self.rasterize_triangle::<false, 0, false>(a, b, c),
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
        let raw_texture = rect.raw_texture;
        let tex_depth = if rect.texcoords.is_some() {
            match self.draw_mode.tex_page().texture_depth() {
                TextureDepth::Bpp15 => 15,
                TextureDepth::Bpp8 => 8,
                TextureDepth::Bpp4 => 4,
                _ => 0,
            }
        } else {
            0
        };

        match (tex_depth, raw_texture) {
            // Modulated with flat color
            (15, false) => self.rasterize_rect::<15, false>(rect),
            (8, false) => self.rasterize_rect::<8, false>(rect),
            (4, false) => self.rasterize_rect::<4, false>(rect),

            // Raw textured
            (15, true) => self.rasterize_rect::<15, true>(rect),
            (8, true) => self.rasterize_rect::<8, true>(rect),
            (4, true) => self.rasterize_rect::<4, true>(rect),

            (_, _) => self.rasterize_rect::<0, false>(rect),
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
            let x = (x + i % w) & (VRAM_WIDTH - 1);
            let y = (y + i / w) & (VRAM_HEIGHT - 1);

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
            let x = (x + i % w) & (VRAM_WIDTH - 1);
            let y = (y + i / w) & (VRAM_HEIGHT - 1);

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
        let prev = mem::take(self);
        self.screen_fill = prev.screen_fill;
    }
}

impl SoftwareRenderer {
    /// SAFETY: all unsafe are contracted by const-generics
    #[inline(never)]
    fn rasterize_triangle<const FLAT_COLOR: bool, const DEPTH: usize, const RAW_TEXTURE: bool>(
        &mut self,
        flat_color: Option<Color>,
        clut: Option<Position>,
        vertices: [Vertex; 3],
    ) {
        debug_assert!(matches!(DEPTH, 0 | 4 | 8 | 15));
        debug_assert!(
            DEPTH == 0 || clut.is_some(),
            "textured polygon must have CLUT"
        );
        debug_assert!(
            DEPTH == 0 || vertices.iter().all(|v| v.texcords.is_some()),
            "textured polygon must have UVs"
        );
        debug_assert!(
            FLAT_COLOR || RAW_TEXTURE || vertices.iter().all(|v| v.color.is_some()),
            "Gouraud polygon must have vertex colors"
        );
        debug_assert!(
            !FLAT_COLOR || RAW_TEXTURE || flat_color.is_some(),
            "flat polygon must have flat color"
        );

        let draw_area = (self.draw_area.0, self.draw_area.1);
        let draw_offset = self.draw_offset;

        let x0 = vertices[0].location.x + draw_offset.x;
        let x1 = vertices[1].location.x + draw_offset.x;
        let x2 = vertices[2].location.x + draw_offset.x;

        let y0 = vertices[0].location.y + draw_offset.y;
        let y1 = vertices[1].location.y + draw_offset.y;
        let y2 = vertices[2].location.y + draw_offset.y;

        let area = cross2(x0, y0, x1, y1, x2, y2);
        if area == 0 {
            return;
        }

        let dx10 = FP::from_num(x1) - FP::from_num(x0);
        let dy10 = FP::from_num(y1) - FP::from_num(y0);
        let dx20 = FP::from_num(x2) - FP::from_num(x0);
        let dy20 = FP::from_num(y2) - FP::from_num(y0);

        let inv_area = FP::ONE.wrapping_div_int(area as i64);

        let (r0, g0, b0, dr_dx, dr_dy, dg_dx, dg_dy, db_dx, db_dy) = if !FLAT_COLOR && !RAW_TEXTURE
        {
            let c0 = unsafe { vertices[0].color.unwrap_unchecked() };
            let c1 = unsafe { vertices[1].color.unwrap_unchecked() };
            let c2 = unsafe { vertices[2].color.unwrap_unchecked() };

            let r0 = FP::from_num(c0.r);
            let r1 = FP::from_num(c1.r);
            let r2 = FP::from_num(c2.r);

            let g0 = FP::from_num(c0.g);
            let g1 = FP::from_num(c1.g);
            let g2 = FP::from_num(c2.g);

            let b0 = FP::from_num(c0.b);
            let b1 = FP::from_num(c1.b);
            let b2 = FP::from_num(c2.b);

            // Color gradients
            let (dr_dx, dr_dy) = {
                let dr10 = r1 - r0;
                let dr20 = r2 - r0;
                (
                    (dr10 * dy20 - dr20 * dy10) * inv_area,
                    (dx10 * dr20 - dx20 * dr10) * inv_area,
                )
            };
            let (dg_dx, dg_dy) = {
                let dg10 = g1 - g0;
                let dg20 = g2 - g0;
                (
                    (dg10 * dy20 - dg20 * dy10) * inv_area,
                    (dx10 * dg20 - dx20 * dg10) * inv_area,
                )
            };
            let (db_dx, db_dy) = {
                let db10 = b1 - b0;
                let db20 = b2 - b0;
                (
                    (db10 * dy20 - db20 * dy10) * inv_area,
                    (dx10 * db20 - dx20 * db10) * inv_area,
                )
            };

            (r0, g0, b0, dr_dx, dr_dy, dg_dx, dg_dy, db_dx, db_dy)
        } else {
            (
                FP::ZERO,
                FP::ZERO,
                FP::ZERO,
                FP::ZERO,
                FP::ZERO,
                FP::ZERO,
                FP::ZERO,
                FP::ZERO,
                FP::ZERO,
            )
        };

        let (clut, u0, v0, du_dx, du_dy, dv_dx, dv_dy) = if DEPTH != 0 {
            let clut = unsafe { clut.unwrap_unchecked() };
            let uv0 = unsafe { vertices[0].texcords.unwrap_unchecked() };
            let uv1 = unsafe { vertices[1].texcords.unwrap_unchecked() };
            let uv2 = unsafe { vertices[2].texcords.unwrap_unchecked() };

            let u0 = FP::from_num(uv0.u);
            let u1 = FP::from_num(uv1.u);
            let u2 = FP::from_num(uv2.u);

            let v0 = FP::from_num(uv0.v);
            let v1 = FP::from_num(uv1.v);
            let v2 = FP::from_num(uv2.v);

            // UV gradients
            let (du_dx, du_dy) = {
                let du10 = u1 - u0;
                let du20 = u2 - u0;
                (
                    (du10 * dy20 - du20 * dy10) * inv_area,
                    (dx10 * du20 - dx20 * du10) * inv_area,
                )
            };
            let (dv_dx, dv_dy) = {
                let dv10 = v1 - v0;
                let dv20 = v2 - v0;
                (
                    (dv10 * dy20 - dv20 * dy10) * inv_area,
                    (dx10 * dv20 - dx20 * dv10) * inv_area,
                )
            };

            (clut, u0, v0, du_dx, du_dy, dv_dx, dv_dy)
        } else {
            (
                Position { x: 0, y: 0 },
                FP::ZERO,
                FP::ZERO,
                FP::ZERO,
                FP::ZERO,
                FP::ZERO,
                FP::ZERO,
            )
        };

        // bounding box (clipped to reduce cycle loops)
        let clip_min_x = (draw_area.0.x as i32).max(0);
        let clip_max_x = (draw_area.1.x as i32).min(VRAM_WIDTH as i32 - 1);
        let clip_min_y = (draw_area.0.y as i32).max(0);
        let clip_max_y = (draw_area.1.y as i32).min(VRAM_HEIGHT as i32 - 1);

        if clip_min_x > clip_max_x || clip_min_y > clip_max_y {
            return;
        }

        let min_x = x0.min(x1).min(x2).max(clip_min_x).min(clip_max_x);
        let max_x = x0.max(x1).max(x2).max(clip_min_x).min(clip_max_x);
        let min_y = y0.min(y1).min(y2).max(clip_min_y).min(clip_max_y);
        let max_y = y0.max(y1).max(y2).max(clip_min_y).min(clip_max_y);

        if min_x > max_x || min_y > max_y {
            return;
        }

        let w0_dx = y1 - y2;
        let w1_dx = y2 - y0;
        let w2_dx = y0 - y1;

        let w0_dy = x2 - x1;
        let w1_dy = x0 - x2;
        let w2_dy = x1 - x0;

        let mut w0_row = cross2(x1, y1, x2, y2, min_x, min_y);
        let mut w1_row = cross2(x2, y2, x0, y0, min_x, min_y);
        let mut w2_row = cross2(x0, y0, x1, y1, min_x, min_y);

        for y in min_y..=max_y {
            let mut w0 = w0_row;
            let mut w1 = w1_row;
            let mut w2 = w2_row;

            let dy = FP::from_num(y) - FP::from_num(y0);
            let dx = FP::from_num(min_x) - FP::from_num(x0);

            let mut r = r0 + dr_dx * dx + dr_dy * dy;
            let mut g = g0 + dg_dx * dx + dg_dy * dy;
            let mut b = b0 + db_dx * dx + db_dy * dy;

            let mut u = u0 + du_dx * dx + du_dy * dy;
            let mut v = v0 + dv_dx * dx + dv_dy * dy;

            for x in min_x..=max_x {
                // Inside test (no backface culling for PSX)
                let inside = if area > 0 {
                    w0 >= 0 && w1 >= 0 && w2 >= 0
                } else {
                    w0 <= 0 && w1 <= 0 && w2 <= 0
                };
                debug_assert_eq!(w0 + w1 + w2, area);

                if inside {
                    let color = if DEPTH != 0 {
                        self.sample_texture::<DEPTH>(
                            clut,
                            u.wrapping_to_num::<u8>(),
                            v.wrapping_to_num::<u8>(),
                        )
                        .map(|texel| {
                            if RAW_TEXTURE {
                                texel
                            } else if FLAT_COLOR {
                                let flat_color = unsafe { flat_color.unwrap_unchecked() };
                                modulate_bgr555(texel, flat_color.r, flat_color.g, flat_color.b)
                            } else {
                                modulate_bgr555(
                                    texel,
                                    fp_to_u8_color(r),
                                    fp_to_u8_color(g),
                                    fp_to_u8_color(b),
                                )
                            }
                        })
                    } else if FLAT_COLOR {
                        let flat_color = unsafe { flat_color.unwrap_unchecked() };
                        Some(rgb888_to_bgr555(flat_color.r, flat_color.g, flat_color.b))
                    } else {
                        Some(rgb888_to_bgr555(
                            fp_to_u8_color(r),
                            fp_to_u8_color(g),
                            fp_to_u8_color(b),
                        ))
                    };

                    if let Some(color) = color {
                        unsafe {
                            *self
                                .vram
                                .get_unchecked_mut(y as usize * VRAM_WIDTH + x as usize) = color;
                        }
                    }
                }

                w0 += w0_dx;
                w1 += w1_dx;
                w2 += w2_dx;

                if !FLAT_COLOR && !RAW_TEXTURE {
                    r += dr_dx;
                    g += dg_dx;
                    b += db_dx;
                }

                if DEPTH != 0 {
                    u += du_dx;
                    v += dv_dx;
                }
            }
            w0_row += w0_dy;
            w1_row += w1_dy;
            w2_row += w2_dy;
        }
    }

    #[inline(never)]
    fn rasterize_rect<const DEPTH: usize, const RAW_TEXTURE: bool>(&mut self, rect: Rect) {
        debug_assert!(matches!(DEPTH, 0 | 4 | 8 | 15));

        debug_assert!(
            DEPTH == 0 || rect.texcoords.is_some(),
            "textured rect must have UV"
        );

        debug_assert!(
            DEPTH == 0 || rect.clut.is_some(),
            "textured rect must have CLUT"
        );

        let draw_area = (self.draw_area.0, self.draw_area.1);
        let draw_offset = self.draw_offset;

        let pos = Location {
            x: rect.location.x + draw_offset.x,
            y: rect.location.y + draw_offset.y,
        };

        let Color { r, g, b } = rect.flat_color;

        let rect_w = rect.size.w as i32;
        let rect_h = rect.size.h as i32;

        if rect_w <= 0 || rect_h <= 0 {
            return;
        }

        let rect_min_x = pos.x;
        let rect_min_y = pos.y;
        let rect_max_x = pos.x + rect_w - 1;
        let rect_max_y = pos.y + rect_h - 1;

        let clip_min_x = (draw_area.0.x as i32).max(0);
        let clip_min_y = (draw_area.0.y as i32).max(0);
        let clip_max_x = (draw_area.1.x as i32).min(VRAM_WIDTH as i32 - 1);
        let clip_max_y = (draw_area.1.y as i32).min(VRAM_HEIGHT as i32 - 1);

        if clip_min_x > clip_max_x || clip_min_y > clip_max_y {
            return;
        }

        let min_x = rect_min_x.max(clip_min_x);
        let min_y = rect_min_y.max(clip_min_y);
        let max_x = rect_max_x.min(clip_max_x);
        let max_y = rect_max_y.min(clip_max_y);

        if min_x > max_x || min_y > max_y {
            return;
        }

        for y in min_y..=max_y {
            let j = (y - pos.y) as u8;

            for x in min_x..=max_x {
                let i = (x - pos.x) as u8;

                let color = if DEPTH != 0 {
                    let uv = unsafe { rect.texcoords.unwrap_unchecked() };

                    let u = if self.draw_mode.texture_rectangle_x_flip() {
                        uv.u.wrapping_sub(i)
                    } else {
                        uv.u.wrapping_add(i)
                    };

                    let v = if self.draw_mode.texture_rectangle_y_flip() {
                        uv.v.wrapping_sub(j)
                    } else {
                        uv.v.wrapping_add(j)
                    };

                    let Some(texel) =
                        self.sample_texture::<DEPTH>(unsafe { rect.clut.unwrap_unchecked() }, u, v)
                    else {
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

    #[inline(always)]
    fn sample_texture<const DEPTH: usize>(&self, clut: Position, u: u8, v: u8) -> Option<u16> {
        debug_assert!(matches!(DEPTH, 0 | 4 | 8 | 15));

        let (base_x, base_y) = (
            self.draw_mode.tex_page().texture_page_x_base() as usize * 64,
            self.draw_mode.tex_page().texture_page_y_base() as usize * 256,
        );
        let (u, v) = self.apply_texture_window(u, v);
        let color = match DEPTH {
            4 => self.fetch_clut_color(clut, self.fetch_index::<4>(base_x, base_y, u, v)),
            8 => self.fetch_clut_color(clut, self.fetch_index::<8>(base_x, base_y, u, v)),
            15 => self.fetch_15bpp(base_x, base_y, u, v),
            _ => unreachable!(),
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
        let x = (base_x + (u as usize / texels_per_pixel)) & (VRAM_WIDTH - 1);
        let y = (base_y + v as usize) & (VRAM_HEIGHT - 1);

        let word = unsafe { *self.vram.get_unchecked(y * VRAM_WIDTH + x) };

        let shift = (u as usize % texels_per_pixel) * BPP;
        ((word >> shift) & ((1 << BPP) - 1)) as u8
    }

    #[inline(always)]
    fn fetch_15bpp(&self, base_x: usize, base_y: usize, u: u8, v: u8) -> u16 {
        let x = (base_x + u as usize) & (VRAM_WIDTH - 1);
        let y = (base_y + v as usize) & (VRAM_HEIGHT - 1);

        unsafe { *self.vram.get_unchecked(y * VRAM_WIDTH + x) }
    }

    #[inline(always)]
    fn fetch_clut_color(&self, clut: Position, index: u8) -> u16 {
        let x = (clut.x + index as usize) & (VRAM_WIDTH - 1);
        let y = clut.y & (VRAM_HEIGHT - 1);

        unsafe { *self.vram.get_unchecked(y * VRAM_WIDTH + x) }
    }
}

#[inline(always)]
fn cross2(ax: i32, ay: i32, bx: i32, by: i32, px: i32, py: i32) -> i32 {
    (bx - ax) * (py - ay) - (by - ay) * (px - ax)
}

#[inline(always)]
fn fp_to_u8_color(v: FP) -> u8 {
    v.clamp(FP::from_num(0), FP::from_num(255)).to_num::<u8>()
}

#[inline(always)]
fn rgb888_to_bgr555(r: u8, g: u8, b: u8) -> u16 {
    let r5 = (r >> 3) as u16;
    let g5 = (g >> 3) as u16;
    let b5 = (b >> 3) as u16;

    r5 | (g5 << 5) | (b5 << 10)
}

#[inline(always)]
fn modulate_bgr555(texel: u16, r: u8, g: u8, b: u8) -> u16 {
    let tr = texel & 0x1F;
    let tg = (texel >> 5) & 0x1F;
    let tb = (texel >> 10) & 0x1F;
    let mask = texel & 0x8000;

    let r = ((tr * r as u16) >> 7).min(0x1F);
    let g = ((tg * g as u16) >> 7).min(0x1F);
    let b = ((tb * b as u16) >> 7).min(0x1F);

    mask | r | (g << 5) | (b << 10)
}
