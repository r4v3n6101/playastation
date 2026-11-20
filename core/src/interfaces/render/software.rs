use alloc::collections::VecDeque;
use core::{mem, num::Wrapping};

use euclid::{
    default::{Box2D, Point2D as Point2, Rect as Rect2, Vector2D as Vec2, Vector3D as Vec3},
    point2, rect, size2, vec2, vec3,
};

use super::{
    Renderer, VRAM_HEIGHT, VRAM_WIDTH,
    types::{
        Color, DrawMode, EnvParameter, MaskBitSetting, Polygon, Polyline, Position, Rect,
        RenderState, SemiTransparency, Size, TextureDepth, TextureWindow, Vertex, Vram,
    },
};

const VRAM_BOUNDS: Box2D<usize> = Box2D::new(point2(0, 0), point2(VRAM_WIDTH, VRAM_HEIGHT));

pub struct SoftwareRenderer {
    vram: Vram,

    draw_mode: DrawMode,
    texture_window: TextureWindow,
    draw_area: Box2D<i32>,
    draw_offset: Vec2<i32>,
    mask_bit_setting: MaskBitSetting,

    to_download: Rect2<usize>,
    pop_counter: u32,

    to_upload: Rect2<usize>,
    push_counter: u32,
}

impl Default for SoftwareRenderer {
    fn default() -> Self {
        Self {
            vram: alloc::vec![0; VRAM_WIDTH * VRAM_HEIGHT].into_boxed_slice(),

            draw_mode: DrawMode::new(),
            texture_window: TextureWindow::new(),
            draw_area: Box2D::new(point2(0, 0), point2(1, 1)),
            draw_offset: vec2(0, 0),
            mask_bit_setting: MaskBitSetting::new(),

            to_download: rect(0, 0, 0, 0),
            pop_counter: 0,

            to_upload: rect(0, 0, 0, 0),
            push_counter: 0,
        }
    }
}

impl Renderer for SoftwareRenderer {
    fn state(&self) -> RenderState {
        RenderState {
            draw_mode: self.draw_mode,
            mask_bit_setting: self.mask_bit_setting,
            vram_read_active: (self.pop_counter as usize) < self.to_download.area(),
        }
    }

    fn framebuffer(&self) -> &[u16] {
        &self.vram
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
                self.draw_area.min = point2(position.x, position.y).max(point2(0, 0)).cast();
            }
            EnvParameter::DrawAreaBottomRight(position) => {
                // GP0 includes the bottom-right pixel; `euclid` boxes exclude max.
                self.draw_area.max =
                    (point2(position.x, position.y).min(VRAM_BOUNDS.max - vec2(1, 1)) + vec2(1, 1))
                        .cast();
            }
            EnvParameter::DrawOffset(location) => {
                self.draw_offset = vec2(location.x, location.y);
            }
            EnvParameter::MaskBitSetting(mask_bit_setting) => {
                self.mask_bit_setting = mask_bit_setting;
            }
        }
    }

    fn draw_polygon(&mut self, polygon: Polygon) {
        if let Some(tex_page) = polygon.tpage {
            self.draw_mode.set_tex_page(tex_page.tex_page());
            self.draw_mode
                .set_texture_disable(tex_page.texture_disable());
        }

        let flat_color = polygon.flat_color.is_some();
        let raw_texture = polygon.raw_texture;
        // 0 means no texture at all
        let tex_depth = if let Some(tpage) = polygon.tpage {
            match tpage.tex_page().texture_depth() {
                TextureDepth::Bpp15 => 15,
                TextureDepth::Bpp8 => 8,
                TextureDepth::Bpp4 => 4,
                TextureDepth::Reserved => 0,
            }
        } else {
            0
        };
        let mut rasterize = |vertices| match (flat_color, tex_depth, raw_texture) {
            // 15BPP, textured, flat
            (true, 15, false) => self.rasterize_triangle::<true, 15, false>(&polygon, vertices),
            // 15BPP, textured, Gouraud
            (false, 15, false) => self.rasterize_triangle::<false, 15, false>(&polygon, vertices),

            // 8BPP, textured, flat
            (true, 8, false) => self.rasterize_triangle::<true, 8, false>(&polygon, vertices),
            // 8BPP, textured, Gouraud
            (false, 8, false) => self.rasterize_triangle::<false, 8, false>(&polygon, vertices),

            // 4BPP, textured, flat
            (true, 4, false) => self.rasterize_triangle::<true, 4, false>(&polygon, vertices),
            // 4BPP, textured, Gouraud
            (false, 4, false) => self.rasterize_triangle::<false, 4, false>(&polygon, vertices),

            // 15BPP, textured, raw texture
            (_, 15, true) => self.rasterize_triangle::<false, 15, true>(&polygon, vertices),
            // 8BPP, textured, raw texture
            (_, 8, true) => self.rasterize_triangle::<false, 8, true>(&polygon, vertices),
            // 4BPP, textured, raw texture
            (_, 4, true) => self.rasterize_triangle::<false, 4, true>(&polygon, vertices),

            // untextured, flat
            (true, _, _) => self.rasterize_triangle::<true, 0, false>(&polygon, vertices),
            // untextured, Gouraud
            (false, _, _) => self.rasterize_triangle::<false, 0, false>(&polygon, vertices),
        };
        match polygon.vertices.as_slice() {
            [a, b, c] => rasterize([*a, *b, *c]),
            [a, b, c, d] => {
                rasterize([*a, *b, *c]);
                rasterize([*b, *c, *d]);
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
                TextureDepth::Reserved => 0,
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

    fn fill_vram_area(&mut self, pos: Position, size: Size, color: Color) {
        let bounds =
            Box2D::from_origin_and_size(point2(pos.x, pos.y), size2(size.w, size.h).cast());

        let Some(bounds) = bounds.intersection(&VRAM_BOUNDS) else {
            return;
        };

        let color = rgb888_to_bgr555(vec3(color.r, color.g, color.b), 0);
        for y in bounds.y_range() {
            for x in bounds.x_range() {
                self.vram[vram_index(point2(x, y))] = color;
            }
        }
    }

    fn prepare_vram_for_read(&mut self, pos: Position, size: Size) {
        self.to_download = rect(pos.x, pos.y, size.w.into(), size.h.into());
        self.pop_counter = 0;
    }

    fn pop_pixel(&mut self) -> Option<u16> {
        let download = self.to_download;
        let i = self.pop_counter as usize;
        if i >= download.area() {
            return None;
        }

        let pos =
            wrap_vram(download.origin + vec2(i % download.size.width, i / download.size.width));
        self.pop_counter += 1;

        Some(self.vram[vram_index(pos)])
    }

    fn prepare_vram_for_write(&mut self, pos: Position, size: Size) {
        self.to_upload = rect(pos.x, pos.y, size.w.into(), size.h.into());
        self.push_counter = 0;
    }

    fn push_pixel(&mut self, pixel: u16) {
        let upload = self.to_upload;
        let i = self.push_counter as usize;
        if i >= upload.area() {
            return;
        }

        let pos = wrap_vram(upload.origin + vec2(i % upload.size.width, i / upload.size.width));
        self.write_pixel(pos, pixel, false);
        self.push_counter += 1;
    }

    fn mirror_vram_area(&mut self, src: Position, dest: Position, size: Size) {
        let size = size2(size.w, size.h).cast::<usize>();
        let src = Box2D::from_origin_and_size(point2(src.x, src.y), size);
        let dest = Box2D::from_origin_and_size(point2(dest.x, dest.y), size);

        // areas may overlap
        let mut tmp = VecDeque::with_capacity(size.area());

        for y in src.y_range() {
            for x in src.x_range() {
                let pos = point2(x, y);
                let pixel = if VRAM_BOUNDS.contains(pos) {
                    self.vram[vram_index(pos)]
                } else {
                    0
                };
                tmp.push_front(pixel);
            }
        }

        for y in dest.y_range() {
            for x in dest.x_range() {
                let pos = point2(x, y);
                let pixel = tmp.pop_back().unwrap();
                if VRAM_BOUNDS.contains(pos) {
                    self.write_pixel(pos, pixel, false);
                }
            }
        }
    }

    fn reset(&mut self) {
        mem::take(self);
    }
}

impl SoftwareRenderer {
    /// SAFETY: all unsafe are contracted by const-generics
    #[inline(never)]
    fn rasterize_triangle<const FLAT_COLOR: bool, const DEPTH: usize, const RAW_TEXTURE: bool>(
        &mut self,
        polygon: &Polygon,
        vertices: [Vertex; 3],
    ) {
        let flat_color = polygon.flat_color.map(|c| vec3(c.r, c.g, c.b));
        let clut = polygon.clut;
        let semi_transparent = polygon.semi_transparent;
        let dither_enabled =
            self.draw_mode.dither_24_to_15() && !RAW_TEXTURE && (!FLAT_COLOR || DEPTH != 0);

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

        let points @ [p0, p1, p2] =
            vertices.map(|v| point2(v.location.x, v.location.y) + self.draw_offset);
        let mut bounds = Box2D::from_points(points);

        // Width/height between extreme vertices; abs_diff avoids signed overflow.
        let span = bounds.min.zip(bounds.max, i32::abs_diff);

        if span.x >= VRAM_WIDTH as u32 || span.y >= VRAM_HEIGHT as u32 {
            return;
        }

        let edges @ [edge10, edge20] = [p1 - p0, p2 - p0];
        let area = edge10.cross(edge20);
        if area == 0 {
            return;
        }

        let [edge10, edge20] = edges.map(|edge| edge.cast::<i64>());
        let area_divisor = i64::from(area);

        // Divide each gradient directly: rounding 1 / area first can turn an
        // exact one-texel step into slightly less than one.
        let interpolate = |a0: u8, a1: u8, a2: u8| {
            let origin = u32::from(a0) << ATTRIBUTE_FRAC_BITS;
            let [da1, da2] = [a1, a2].map(|a| i64::from(a) - i64::from(a0));

            // Keep products and signed division wide; wrap only the finished steps.
            let gradient = vec2(
                da1 * edge20.y - da2 * edge10.y,
                edge10.x * da2 - edge20.x * da1,
            )
            .map(|n| ((n << ATTRIBUTE_FRAC_BITS) / area_divisor) as u32);

            AttributePlane { origin, gradient }
        };

        let color_planes = if !FLAT_COLOR && !RAW_TEXTURE {
            let [c0, c1, c2] = vertices.map(|v| unsafe { v.color.unwrap_unchecked() });
            [
                interpolate(c0.r, c1.r, c2.r),
                interpolate(c0.g, c1.g, c2.g),
                interpolate(c0.b, c1.b, c2.b),
            ]
        } else {
            [AttributePlane::default(); 3]
        };

        let (clut, uv_planes) = if DEPTH != 0 {
            let clut = unsafe { clut.unwrap_unchecked() };
            let [uv0, uv1, uv2] = vertices.map(|v| unsafe { v.texcords.unwrap_unchecked() });
            (
                point2(clut.x, clut.y),
                [
                    interpolate(uv0.u, uv1.u, uv2.u),
                    interpolate(uv0.v, uv1.v, uv2.v),
                ],
            )
        } else {
            (point2(0, 0), [AttributePlane::default(); 2])
        };

        // Include the last integer coordinate in euclid's half-open box.
        bounds.max = bounds.max.map(|v| v.saturating_add(1));
        let Some(bounds) = bounds.intersection(&self.draw_area) else {
            return;
        };

        let winding = area.signum();
        let edges = [p2 - p1, p0 - p2, p1 - p0].map(|edge| edge * winding);

        // Include only top/left edges so shared edges are blended exactly once.
        let edge_min = edges.map(|edge| i32::from(!is_top_left_edge(edge)));
        let edge_dx = edges.map(|edge| -edge.y);
        let edge_dy = edges.map(|edge| edge.x);

        let mut edge_row = [
            edges[0].cross(bounds.min - p1),
            edges[1].cross(bounds.min - p2),
            edges[2].cross(bounds.min - p0),
        ];
        let color_dx: Vec3<_> = color_planes.map(|plane| plane.gradient.x).into();
        let uv_dx: Vec2<_> = uv_planes.map(|plane| plane.gradient.x).into();

        for y in bounds.y_range() {
            let mut weights = edge_row;

            let offset = point2(bounds.min.x, y) - p0;
            let mut rgb: Vec3<_> = color_planes.map(|plane| plane.at(offset)).into();
            let mut uv: Vec2<_> = uv_planes.map(|plane| plane.at(offset)).into();

            for x in bounds.x_range() {
                // Edge functions are normalized for either winding (no backface culling).
                let inside = weights[0] >= edge_min[0]
                    && weights[1] >= edge_min[1]
                    && weights[2] >= edge_min[2];
                debug_assert_eq!(weights[0] + weights[1] + weights[2], area.abs());

                if inside {
                    let pos = point2(x, y);
                    let dither = if dither_enabled {
                        dither_offset(pos)
                    } else {
                        0
                    };
                    let color = if DEPTH != 0 {
                        self.sample_texture::<DEPTH>(clut, uv.map(attribute_to_u8))
                            .map(|texel| {
                                if RAW_TEXTURE {
                                    texel
                                } else if FLAT_COLOR {
                                    let flat_color = unsafe { flat_color.unwrap_unchecked() };
                                    modulate_bgr555(texel, flat_color, dither)
                                } else {
                                    modulate_bgr555(texel, rgb.map(attribute_to_u8), dither)
                                }
                            })
                    } else if FLAT_COLOR {
                        let flat_color = unsafe { flat_color.unwrap_unchecked() };
                        Some(rgb888_to_bgr555(flat_color, 0))
                    } else {
                        Some(rgb888_to_bgr555(rgb.map(attribute_to_u8), dither))
                    };

                    if let Some(color) = color {
                        self.write_pixel(
                            pos.cast::<usize>(),
                            color,
                            semi_transparent && (DEPTH == 0 || color & 0x8000 != 0),
                        );
                    }
                }

                for i in 0..3 {
                    weights[i] += edge_dx[i];
                }

                if !FLAT_COLOR && !RAW_TEXTURE {
                    rgb = rgb.zip(color_dx, u32::wrapping_add);
                }

                if DEPTH != 0 {
                    uv = uv.zip(uv_dx, u32::wrapping_add);
                }
            }

            for i in 0..3 {
                edge_row[i] += edge_dy[i];
            }
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

        let origin = point2(rect.location.x, rect.location.y) + self.draw_offset;
        let bounds = Box2D::from_origin_and_size(origin, size2(rect.size.w, rect.size.h).cast());
        let Some(bounds) = bounds.intersection(&self.draw_area) else {
            return;
        };

        let color = rect.flat_color;
        let rgb = vec3(color.r, color.g, color.b);
        let uv_step = vec2(
            self.draw_mode.texture_rectangle_x_flip(),
            self.draw_mode.texture_rectangle_y_flip(),
        )
        .map(|flip| if flip { -1 } else { 1 });

        for y in bounds.y_range() {
            for x in bounds.x_range() {
                let pos = point2(x, y);

                let color = if DEPTH != 0 {
                    let uv = unsafe { rect.texcoords.unwrap_unchecked() };

                    // Sprite offsets fit in i32; only the final UVs wrap to 8 bits.
                    let offset = (pos - origin).component_mul(uv_step);
                    let uv = vec2(uv.u, uv.v).cast::<i32>() + offset;
                    let uv = uv.map(|v| v & 0xFF).cast::<u8>();
                    let clut = unsafe { rect.clut.unwrap_unchecked() };
                    let Some(texel) = self.sample_texture::<DEPTH>(point2(clut.x, clut.y), uv)
                    else {
                        continue;
                    };

                    if RAW_TEXTURE {
                        texel
                    } else {
                        modulate_bgr555(texel, rgb, 0)
                    }
                } else {
                    rgb888_to_bgr555(rgb, 0)
                };

                self.write_pixel(
                    pos.cast::<usize>(),
                    color,
                    rect.semi_transparent && (DEPTH == 0 || color & 0x8000 != 0),
                );
            }
        }
    }

    #[inline(always)]
    fn write_pixel(&mut self, pos: Point2<usize>, mut color: u16, semi_transparent: bool) {
        let pixel = &mut self.vram[vram_index(pos)];

        if self.mask_bit_setting.check_mask_before_drawing() && *pixel & 0x8000 != 0 {
            return;
        }

        if semi_transparent {
            color = blend_bgr555(*pixel, color, self.draw_mode.tex_page().semi_transparency());
        }
        if self.mask_bit_setting.set_mask_while_drawing() {
            color |= 0x8000;
        }

        *pixel = color;
    }

    #[inline(always)]
    fn sample_texture<const DEPTH: usize>(&self, clut: Point2<usize>, uv: Vec2<u8>) -> Option<u16> {
        debug_assert!(matches!(DEPTH, 0 | 4 | 8 | 15));

        let page = self.draw_mode.tex_page();
        let base = point2(
            u16::from(page.texture_page_x_base()) * 64,
            u16::from(page.texture_page_y_base()) * 256,
        )
        .cast::<usize>();
        let uv = self.apply_texture_window(uv).cast::<usize>();
        let color = match DEPTH {
            4 => self.fetch_clut_color(clut, self.fetch_index::<4>(base, uv)),
            8 => self.fetch_clut_color(clut, self.fetch_index::<8>(base, uv)),
            15 => self.fetch_15bpp(base, uv),
            _ => unreachable!(),
        };

        // Only 0x0000 is transparent; 0x8000 is black with the STP bit set.
        (color != 0).then_some(color)
    }

    #[inline(always)]
    fn apply_texture_window(&self, uv: Vec2<u8>) -> Vec2<u8> {
        let mask_x = self.texture_window.mask_x() << 3;
        let mask_y = self.texture_window.mask_y() << 3;
        let offset_x = (self.texture_window.offset_x() << 3) & mask_x;
        let offset_y = (self.texture_window.offset_y() << 3) & mask_y;

        vec2((uv.x & !mask_x) | offset_x, (uv.y & !mask_y) | offset_y)
    }

    #[inline(always)]
    fn fetch_index<const BPP: usize>(&self, base: Point2<usize>, uv: Vec2<usize>) -> u8 {
        let texels_per_pixel = 16 / BPP;
        let pos = wrap_vram(base + vec2(uv.x / texels_per_pixel, uv.y));
        let word = unsafe { *self.vram.get_unchecked(vram_index(pos)) };

        let shift = (uv.x % texels_per_pixel) * BPP;
        ((word >> shift) & ((1 << BPP) - 1)) as u8
    }

    #[inline(always)]
    fn fetch_15bpp(&self, base: Point2<usize>, uv: Vec2<usize>) -> u16 {
        let pos = wrap_vram(base + uv);
        unsafe { *self.vram.get_unchecked(vram_index(pos)) }
    }

    #[inline(always)]
    fn fetch_clut_color(&self, clut: Point2<usize>, index: u8) -> u16 {
        let pos = wrap_vram(clut + vec2(usize::from(index), 0));
        unsafe { *self.vram.get_unchecked(vram_index(pos)) }
    }
}

#[inline(always)]
fn wrap_vram(pos: Point2<usize>) -> Point2<usize> {
    point2(pos.x & (VRAM_WIDTH - 1), pos.y & (VRAM_HEIGHT - 1))
}

#[inline(always)]
fn vram_index(pos: Point2<usize>) -> usize {
    pos.y * VRAM_WIDTH + pos.x
}

// RGB/UV gradients are stored in units of 1/4096.
// Each truncated gradient contributes < 1/4096 error per pixel on its axis.
// At most 1023 horizontal and 511 vertical steps keep the accumulated error
// below 0.375 of a color level or texel, before rounding to u8.
const ATTRIBUTE_FRAC_BITS: u32 = 12;

#[derive(Clone, Copy, Default)]
struct AttributePlane {
    origin: u32,
    gradient: Vec2<u32>,
}

impl AttributePlane {
    #[inline(always)]
    fn at(self, offset: Vec2<i32>) -> u32 {
        let offset = offset.map(|v| Wrapping(v.cast_unsigned()));
        // Round attributes to the nearest texel/color, without shifting coverage.
        let rounding_bias = 1 << (ATTRIBUTE_FRAC_BITS - 1);
        // Negative slopes and extrapolation wrap; sampling keeps only a byte.
        (self.origin + rounding_bias).wrapping_add(self.gradient.map(Wrapping).dot(offset).0)
    }
}

#[inline(always)]
fn is_top_left_edge(edge: Vec2<i32>) -> bool {
    edge.y < 0 || (edge.y == 0 && edge.x > 0)
}

#[inline(always)]
fn blend_bgr555(background: u16, foreground: u16, mode: SemiTransparency) -> u16 {
    let back = unpack_bgr555(background);
    let front = unpack_bgr555(foreground);
    let color = match mode {
        SemiTransparency::Average => back.zip(front, u16::midpoint),
        SemiTransparency::Add => (back + front).min(Vec3::splat(0x1F)),
        SemiTransparency::Subtract => back.zip(front, u16::saturating_sub),
        SemiTransparency::AddQuarter => (back + front / 4).min(Vec3::splat(0x1F)),
    };

    (foreground & 0x8000) | pack_bgr555(color)
}

#[inline(always)]
fn unpack_bgr555(color: u16) -> Vec3<u16> {
    vec3(color, color >> 5, color >> 10).map(|c| c & 0x1F)
}

#[inline(always)]
fn pack_bgr555(color: Vec3<u16>) -> u16 {
    color.x | (color.y << 5) | (color.z << 10)
}

#[inline(always)]
fn attribute_to_u8(value: u32) -> u8 {
    (value >> ATTRIBUTE_FRAC_BITS) as u8
}

#[inline(always)]
fn rgb888_to_bgr555(rgb: Vec3<u8>, dither: i16) -> u16 {
    pack_bgr555(rgb.cast::<i16>().map(|c| quantize_channel(c, dither)))
}

#[inline(always)]
fn modulate_bgr555(texel: u16, rgb: Vec3<u8>, dither: i16) -> u16 {
    // Keep the low three color bits until after dithering; modulation can exceed 255.
    // Each product is at most 31 * 255 = 7905, so u16 retains the full precision.
    let product = unpack_bgr555(texel).component_mul(rgb.cast::<u16>());
    let color = product.map(|c| c >> 4).cast::<i16>();
    let color = color.map(|c| quantize_channel(c, dither));

    (texel & 0x8000) | pack_bgr555(color)
}

#[inline(always)]
fn quantize_channel(color: i16, dither: i16) -> u16 {
    ((color + dither).clamp(0, 255) >> 3).cast_unsigned()
}

#[inline(always)]
fn dither_offset(pos: Point2<i32>) -> i16 {
    // The pattern is anchored to VRAM, after applying the drawing offset.
    const MATRIX: [[i16; 4]; 4] = [
        [-4, 0, -3, 1],
        [2, -2, 3, -1],
        [-3, 1, -4, 0],
        [3, -1, 2, -2],
    ];

    let pos = pos.map(|v| v & 3).cast::<usize>();
    MATRIX[pos.y][pos.x]
}
