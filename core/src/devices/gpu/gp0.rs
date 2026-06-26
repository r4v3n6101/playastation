use core::{fmt, mem};

use smallbox::{SmallBox, space::S32};
use smallvec::SmallVec;

use crate::render::types::{
    Color, DrawMode, EnvParameter, Location, MaskBitSetting, POLYGON_STACK_LIMIT,
    POLYLINE_STACK_LIMIT, Polygon, Polyline, Position, Rect, Size, TexturePage, TextureWindow, UV,
    Vertex,
};

use super::Gpu;

#[derive(Debug)]
pub struct CmdBuf(SmallBox<dyn PacketBuilder, S32>);

/// [`Default`] state is like after NOP command.
impl Default for CmdBuf {
    fn default() -> Self {
        Self(SmallBox::new(()))
    }
}

#[tracing::instrument(
    target = "gpu.gp0",
    level = "DEBUG",
    "dispatch",
    skip(gpu),
    fields(cmd=%format_args!("{cmd:#X}"))
)]
pub fn dispatch(gpu: &mut Gpu, cmd: u32) {
    let mut cmdbuf = mem::take(&mut gpu.cmdbuf);

    if cmdbuf.0.needs_more() {
        tracing::trace!("push cmd as is");
        cmdbuf.0.push_cmd(cmd, gpu);
    } else {
        let opcode = (cmd >> 24) as u8;

        cmdbuf.0 = SmallBox::new(());
        match opcode {
            0x00 | 0x03..=0x1E => {
                // NOP
            }
            0x01 => {
                // Clear CLUT AFAIK
            }
            0x1F => {
                gpu.int_flag = true;
            }
            0x02 => {
                cmdbuf.0 = SmallBox::new(FillVramPacket::init(cmd));
            }
            0x20..=0x3F => {
                cmdbuf.0 = SmallBox::new(PolygonPacket::init(cmd));
            }
            0x40..=0x5F => {
                cmdbuf.0 = SmallBox::new(LinePacket::init(cmd));
            }
            0x60..=0x7F => {
                cmdbuf.0 = SmallBox::new(RectPacket::init(cmd));
            }
            0x80 => {
                cmdbuf.0 = SmallBox::new(Vram2VramPacket::init(cmd));
            }
            0xA0 => {
                cmdbuf.0 = SmallBox::new(Cpu2VramPacket::init(cmd));
            }
            0xC0 => {
                cmdbuf.0 = SmallBox::new(Vram2CpuPacket::init(cmd));
            }
            0xE1 => set_draw_mode(gpu, cmd),
            0xE2 => set_texture_window(gpu, cmd),
            0xE3 => set_draw_area_top_left(gpu, cmd),
            0xE4 => set_draw_area_bottom_right(gpu, cmd),
            0xE5 => set_draw_offset(gpu, cmd),
            0xE6 => set_mask_bit_setting(gpu, cmd),
            _ => {}
        }
    }

    if !cmdbuf.0.needs_more() {
        tracing::debug!(packet=?cmdbuf.0, "packet gathered");
        cmdbuf.0.commit(gpu);
    } else {
        gpu.cmdbuf = cmdbuf;
    }
}

#[tracing::instrument(target = "gpu.gp0", level = "DEBUG", "gpuread", skip(gpu))]
pub fn read(gpu: &mut Gpu) -> u32 {
    let mut data = [0u32; 2];
    for pixel in &mut data {
        if let Some(data) = gpu.renderer.pop_pixel() {
            *pixel = u32::from(data);
        }
    }

    data[1] << 16 | data[0]
}

trait PacketBuilder: fmt::Debug {
    fn init(cmd: u32) -> Self
    where
        Self: Sized;

    fn push_cmd(&mut self, cmd: u32, gpu: &mut Gpu);

    fn needs_more(&self) -> bool;

    fn commit(&mut self, gpu: &mut Gpu);
}

#[derive(Debug)]
struct PolygonPacket {
    gouraud: bool,
    textured: bool,
    raw_texture: bool,
    semi_transparent: bool,

    color: Option<Color>,
    vertices: SmallVec<[VertexBuilder; POLYGON_STACK_LIMIT]>,
    clut: Option<Position>,
    tpage: Option<TexturePage>,

    words_left: usize,
}

#[derive(Debug)]
struct LinePacket {
    gouraud: bool,

    color: Option<Color>,
    vertices: SmallVec<[VertexBuilder; POLYLINE_STACK_LIMIT]>,

    /// [`Option::None`] when awaiting for terminator
    words_left: Option<usize>,
}

#[derive(Debug)]
struct RectPacket {
    textured: bool,
    raw_texture: bool,
    semi_transparent: bool,

    color: Color,
    loc: Option<Location>,
    uv: Option<UV>,
    clut: Option<Position>,
    size: Option<Size>,

    words_left: usize,
}

#[derive(Debug)]
struct FillVramPacket {
    color: Color,
    pos: Option<Position>,
    size: Option<Size>,
}

#[derive(Debug)]
struct Cpu2VramPacket {
    pos: Option<Position>,
    size: Option<Size>,

    pixels_written: u32,
}

#[derive(Debug)]
struct Vram2CpuPacket {
    pos: Option<Position>,
    size: Option<Size>,
}

#[derive(Debug)]
struct Vram2VramPacket {
    src: Option<Position>,
    dest: Option<Position>,
    size: Option<Size>,
}

#[derive(Debug, Default)]
struct VertexBuilder {
    loc: Option<Location>,
    color: Option<Color>,
    uv: Option<UV>,
}

impl PacketBuilder for () {
    fn init(_: u32) -> Self
    where
        Self: Sized,
    {
    }

    fn push_cmd(&mut self, _: u32, _: &mut Gpu) {}

    fn needs_more(&self) -> bool {
        false
    }

    fn commit(&mut self, _: &mut Gpu) {}
}

impl PacketBuilder for PolygonPacket {
    fn init(cmd: u32) -> Self
    where
        Self: Sized,
    {
        let op = (cmd >> 24) as u8;
        let quad = (op & 0x08) != 0;
        let gouraud = (op & 0x10) != 0;
        let textured = (op & 0x04) != 0;
        let raw_texture = textured && (op & 0x01) != 0;
        let semi_transparent = (op & 0x02) != 0;

        let mut vertices = SmallVec::new();
        let color = parse_color(cmd);
        let color = if !gouraud {
            Some(color)
        } else {
            vertices.push(VertexBuilder {
                color: Some(color),
                ..Default::default()
            });

            None
        };

        // The first color is in initial word
        let words_left = match (quad, gouraud, textured) {
            // 3 vertices
            (false, false, false) => 3,
            // 3 vertices + 2 colors
            (false, true, false) => 5,
            // 3 vertices + 3 uv-s
            (false, false, true) => 6,
            // 3 vertices + 2 colors + 3 uv-s
            (false, true, true) => 8,

            // 4 vertices
            (true, false, false) => 4,
            // 4 vertices + 3 colors
            (true, true, false) => 7,
            // 4 vertices + 4 uv-s
            (true, false, true) => 8,
            // 4 vertices + 3 colors + 4 uv-s
            (true, true, true) => 11,
        };

        Self {
            gouraud,
            textured,
            raw_texture,
            semi_transparent,

            color,
            vertices,
            clut: None,
            tpage: None,

            words_left,
        }
    }

    fn push_cmd(&mut self, cmd: u32, _: &mut Gpu) {
        self.words_left -= 1;

        loop {
            if let Some(last) = self.vertices.last_mut() {
                if self.gouraud
                    && let color @ None = &mut last.color
                {
                    color.replace(parse_color(cmd));
                    return;
                }
                if let loc @ None = &mut last.loc {
                    loc.replace(parse_loc(cmd));
                    return;
                }
                if self.textured
                    && let uv @ None = &mut last.uv
                {
                    if let clut @ None = &mut self.clut {
                        let parsed = parse_uv_clut(cmd);
                        uv.replace(parsed.0);
                        clut.replace(parsed.1);
                    } else if let tpage @ None = &mut self.tpage {
                        let parsed = parse_uv_tpage(cmd);
                        uv.replace(parsed.0);
                        tpage.replace(parsed.1);
                    } else {
                        uv.replace(parse_uv(cmd));
                    }
                    return;
                }
            }
            self.vertices.push(VertexBuilder::default());
        }
    }

    fn needs_more(&self) -> bool {
        self.words_left > 0
    }

    fn commit(&mut self, gpu: &mut Gpu) {
        gpu.renderer.draw_polygon(Polygon {
            vertices: self
                .vertices
                .iter()
                .map(|b| Vertex {
                    location: b.loc.unwrap(),
                    color: b.color,
                    texcords: b.uv,
                })
                .collect(),
            raw_texture: self.raw_texture,
            semi_transparent: self.semi_transparent,
            flat_color: self.color,
            clut: self.clut,
            tpage: self.tpage,
        });
    }
}

impl PacketBuilder for LinePacket {
    fn init(cmd: u32) -> Self
    where
        Self: Sized,
    {
        let op = (cmd >> 24) as u8;
        let polyline = (op & 0x08) != 0;
        let gouraud = (op & 0x10) != 0;

        let mut vertices = SmallVec::new();
        let color = parse_color(cmd);
        let color = if !gouraud {
            Some(color)
        } else {
            vertices.push(VertexBuilder {
                color: Some(color),
                ..Default::default()
            });
            None
        };

        // The first color is in initial word
        let words_left = match (polyline, gouraud) {
            // 2 vertices
            (false, false) => Some(2),
            // 2 vertices + color
            (false, true) => Some(3),
            // Until terminator
            (true, _) => None,
        };

        Self {
            gouraud,

            color,
            vertices,

            words_left,
        }
    }

    fn push_cmd(&mut self, cmd: u32, _: &mut Gpu) {
        const TERMINATOR_CMD: u32 = 0x5000_5000;

        if cmd == TERMINATOR_CMD {
            self.words_left = Some(0);
            return;
        } else if let Some(words_left) = &mut self.words_left {
            *words_left -= 1;
        }

        loop {
            if let Some(last) = self.vertices.last_mut() {
                if self.gouraud
                    && let color @ None = &mut last.color
                {
                    color.replace(parse_color(cmd));
                    return;
                }
                if let loc @ None = &mut last.loc {
                    loc.replace(parse_loc(cmd));
                    return;
                }
            }

            self.vertices.push(VertexBuilder::default());
        }
    }

    fn needs_more(&self) -> bool {
        self.words_left != Some(0)
    }

    fn commit(&mut self, gpu: &mut Gpu) {
        gpu.renderer.draw_polyline(Polyline {
            vertices: self
                .vertices
                .iter()
                .map(|b| Vertex {
                    location: b.loc.unwrap(),
                    color: b.color,
                    texcords: b.uv,
                })
                .collect(),
            flat_color: self.color,
        });
    }
}

impl PacketBuilder for RectPacket {
    fn init(cmd: u32) -> Self
    where
        Self: Sized,
    {
        let op = (cmd >> 24) as u8;
        let raw_texture = (op & 0x01) != 0;
        let semi_transparent = (op & 0x02) != 0;
        let textured = (op & 0x04) != 0;

        let color = parse_color(cmd);
        let size = match op & 0x18 {
            // Variable sized
            0x00 => None,
            // Dot (1x1)
            0x08 => Some(Size { w: 1, h: 1 }),
            // Quad (8x8)
            0x10 => Some(Size { w: 8, h: 8 }),
            // Quad (16x16)
            0x18 => Some(Size { w: 16, h: 16 }),
            _ => unreachable!(),
        };

        let words_left = match (textured, size.is_some()) {
            // loc + size
            (false, false) => 2,
            // loc
            (false, true) => 1,

            // loc + uv/clut + size
            (true, false) => 3,
            // loc + uv/clut
            (true, true) => 2,
        };

        Self {
            textured,
            raw_texture,
            semi_transparent,

            color,
            loc: None,
            uv: None,
            clut: None,
            size,

            words_left,
        }
    }

    fn push_cmd(&mut self, cmd: u32, _: &mut Gpu) {
        self.words_left -= 1;

        if let loc @ None = &mut self.loc {
            loc.replace(parse_loc(cmd));
            return;
        }
        if self.textured
            && let uv @ None = &mut self.uv
        {
            let parsed = parse_uv_clut(cmd);
            uv.replace(parsed.0);
            self.clut.replace(parsed.1);
            return;
        }

        self.size.replace(parse_size(cmd));
    }

    fn needs_more(&self) -> bool {
        self.words_left > 0
    }

    fn commit(&mut self, gpu: &mut Gpu) {
        gpu.renderer.draw_rect(Rect {
            location: self.loc.unwrap(),
            size: self.size.unwrap(),
            raw_texture: self.raw_texture,
            semi_transparent: self.semi_transparent,
            flat_color: self.color,
            texcoords: self.uv,
            clut: self.clut,
        });
    }
}

impl PacketBuilder for FillVramPacket {
    fn init(cmd: u32) -> Self
    where
        Self: Sized,
    {
        Self {
            color: parse_color(cmd),
            pos: None,
            size: None,
        }
    }

    fn push_cmd(&mut self, cmd: u32, _: &mut Gpu) {
        if let pos @ None = &mut self.pos {
            pos.replace(parse_pos(cmd));
            return;
        }
        self.size.replace(parse_size(cmd));
    }

    fn needs_more(&self) -> bool {
        self.size.is_none()
    }

    fn commit(&mut self, gpu: &mut Gpu) {
        gpu.renderer
            .fill_vram_area(self.pos.unwrap(), self.size.unwrap(), self.color);
    }
}

impl PacketBuilder for Cpu2VramPacket {
    fn init(_: u32) -> Self
    where
        Self: Sized,
    {
        Self {
            pos: None,
            size: None,
            pixels_written: 0,
        }
    }

    fn push_cmd(&mut self, cmd: u32, gpu: &mut Gpu) {
        if let pos @ None = &mut self.pos {
            pos.replace(parse_pos(cmd));
            return;
        }
        match &mut self.size {
            size @ None => {
                let sz = parse_size(cmd);
                size.replace(sz);

                if sz.w > 0 && sz.h > 0 {
                    gpu.renderer.prepare_vram_for_write(self.pos.unwrap(), sz);
                }
            }
            Some(size) => {
                debug_assert!(self.pixels_written < u32::from(size.w) * u32::from(size.h));

                for pixel in [cmd as u16, (cmd >> 16) as u16] {
                    gpu.renderer.push_pixel(pixel);
                    self.pixels_written = self.pixels_written.saturating_add(1);
                }
            }
        }
    }

    fn needs_more(&self) -> bool {
        let Some(size) = self.size else {
            return true;
        };
        let size = u32::from(size.w) * u32::from(size.h);

        self.pixels_written < size
    }

    fn commit(&mut self, _: &mut Gpu) {}
}

impl PacketBuilder for Vram2CpuPacket {
    fn init(_: u32) -> Self
    where
        Self: Sized,
    {
        Self {
            pos: None,
            size: None,
        }
    }

    fn push_cmd(&mut self, cmd: u32, _: &mut Gpu) {
        if let pos @ None = &mut self.pos {
            pos.replace(parse_pos(cmd));
            return;
        }
        self.size.replace(parse_size(cmd));
    }

    fn needs_more(&self) -> bool {
        self.size.is_none()
    }

    fn commit(&mut self, gpu: &mut Gpu) {
        gpu.renderer
            .prepare_vram_for_read(self.pos.unwrap(), self.size.unwrap());
    }
}

impl PacketBuilder for Vram2VramPacket {
    fn init(_: u32) -> Self
    where
        Self: Sized,
    {
        Self {
            src: None,
            dest: None,
            size: None,
        }
    }

    fn push_cmd(&mut self, cmd: u32, _: &mut Gpu) {
        if let src @ None = &mut self.src {
            src.replace(parse_pos(cmd));
            return;
        }
        if let dest @ None = &mut self.dest {
            dest.replace(parse_pos(cmd));
            return;
        }
        self.size.replace(parse_size(cmd));
    }

    fn needs_more(&self) -> bool {
        self.size.is_none()
    }

    fn commit(&mut self, gpu: &mut Gpu) {
        gpu.renderer
            .mirror_vram_area(self.src.unwrap(), self.dest.unwrap(), self.size.unwrap());
    }
}

fn set_draw_mode(gpu: &mut Gpu, cmd: u32) {
    gpu.renderer
        .set_parameter(EnvParameter::DrawMode(DrawMode::from_bytes(
            (cmd as u16).to_le_bytes(),
        )));
}

fn set_texture_window(gpu: &mut Gpu, cmd: u32) {
    let bytes = cmd.to_le_bytes();
    gpu.renderer
        .set_parameter(EnvParameter::TextureWindow(TextureWindow::from_bytes([
            bytes[0], bytes[1], bytes[2],
        ])));
}

fn set_draw_area_top_left(gpu: &mut Gpu, cmd: u32) {
    gpu.renderer
        .set_parameter(EnvParameter::DrawAreaTopLeft(Position {
            x: (cmd & 0x03ff) as usize,
            y: ((cmd >> 10) & 0x01ff) as usize,
        }));
}

fn set_draw_area_bottom_right(gpu: &mut Gpu, cmd: u32) {
    gpu.renderer
        .set_parameter(EnvParameter::DrawAreaBottomRight(Position {
            x: (cmd & 0x03ff) as usize,
            y: ((cmd >> 10) & 0x01ff) as usize,
        }));
}

fn set_draw_offset(gpu: &mut Gpu, cmd: u32) {
    fn sign_extend_11(v: u32) -> i32 {
        (v << 21) as i32 >> 21
    }

    gpu.renderer
        .set_parameter(EnvParameter::DrawOffset(Location {
            x: sign_extend_11(cmd & 0x07ff),
            y: sign_extend_11((cmd >> 11) & 0x07ff),
        }));
}

fn set_mask_bit_setting(gpu: &mut Gpu, cmd: u32) {
    let bytes = (cmd as u8).to_le_bytes();
    gpu.renderer
        .set_parameter(EnvParameter::MaskBitSetting(MaskBitSetting::from_bytes(
            bytes,
        )));
}

fn parse_color(cmd: u32) -> Color {
    Color {
        r: cmd as u8,
        g: (cmd >> 8) as u8,
        b: (cmd >> 16) as u8,
    }
}

fn parse_loc(cmd: u32) -> Location {
    Location {
        x: (cmd & 0xFFFF) as i16 as i32,
        y: ((cmd >> 16) & 0xFFFF) as i16 as i32,
    }
}

fn parse_pos(cmd: u32) -> Position {
    Position {
        x: (cmd & 0xFFFF) as usize,
        y: ((cmd >> 16) & 0xFFFF) as usize,
    }
}

fn parse_size(cmd: u32) -> Size {
    Size {
        w: cmd as u16,
        h: (cmd >> 16) as u16,
    }
}

fn parse_uv_clut(cmd: u32) -> (UV, Position) {
    let raw = cmd >> 16;
    let x = ((raw & 0x3f) as usize) * 16;
    let y = ((raw >> 6) & 0x1ff) as usize;

    (
        UV {
            u: cmd as u8,
            v: (cmd >> 8) as u8,
        },
        Position { x, y },
    )
}

fn parse_uv_tpage(cmd: u32) -> (UV, TexturePage) {
    (
        UV {
            u: cmd as u8,
            v: (cmd >> 8) as u8,
        },
        TexturePage::from_bytes(((cmd >> 16) as u16).to_le_bytes()),
    )
}

fn parse_uv(cmd: u32) -> UV {
    UV {
        u: cmd as u8,
        v: (cmd >> 8) as u8,
    }
}

#[cfg(test)]
mod tests {
    use crate::render::types::{Color, Location, Size};

    use super::{super::Gpu, LinePacket, PacketBuilder, PolygonPacket, RectPacket};

    fn loc(x: i16, y: i16) -> u32 {
        u32::from(x.cast_unsigned()) | (u32::from(y.cast_unsigned()) << 16)
    }

    fn rgb(r: u8, g: u8, b: u8) -> u32 {
        u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)
    }

    #[test]
    fn builds_monochrome_triangle_packet() {
        let mut gpu = Gpu::default();
        let mut packet = PolygonPacket::init(0x2000_0000 | rgb(0x11, 0x22, 0x33));

        assert!(packet.needs_more());
        assert!(!packet.gouraud);
        assert!(!packet.textured);
        assert_eq!(
            packet.color,
            Some(Color {
                r: 0x11,
                g: 0x22,
                b: 0x33,
            })
        );
        assert_eq!(packet.vertices.len(), 0);

        packet.push_cmd(loc(1, 2), &mut gpu);
        packet.push_cmd(loc(-3, 4), &mut gpu);
        packet.push_cmd(loc(5, -6), &mut gpu);

        assert!(!packet.needs_more());
        assert_eq!(packet.vertices.len(), 3);
        assert_eq!(packet.vertices[0].loc, Some(Location { x: 1, y: 2 }));
        assert_eq!(packet.vertices[1].loc, Some(Location { x: -3, y: 4 }));
        assert_eq!(packet.vertices[2].loc, Some(Location { x: 5, y: -6 }));
    }

    #[test]
    fn builds_gouraud_quad_packet() {
        let mut gpu = Gpu::default();
        let mut packet = PolygonPacket::init(0x3800_0000 | rgb(0x10, 0x20, 0x30));

        assert!(packet.gouraud);
        assert!(!packet.textured);
        assert_eq!(packet.color, None);
        assert_eq!(packet.vertices.len(), 1);
        assert_eq!(
            packet.vertices[0].color,
            Some(Color {
                r: 0x10,
                g: 0x20,
                b: 0x30,
            })
        );

        packet.push_cmd(loc(1, 2), &mut gpu);
        packet.push_cmd(rgb(0x40, 0x50, 0x60), &mut gpu);
        packet.push_cmd(loc(3, 4), &mut gpu);
        packet.push_cmd(rgb(0x70, 0x80, 0x90), &mut gpu);
        packet.push_cmd(loc(5, 6), &mut gpu);
        packet.push_cmd(rgb(0xA0, 0xB0, 0xC0), &mut gpu);
        packet.push_cmd(loc(7, 8), &mut gpu);

        assert!(!packet.needs_more());
        assert_eq!(packet.vertices.len(), 4);
        assert_eq!(packet.vertices[0].loc, Some(Location { x: 1, y: 2 }));
        assert_eq!(packet.vertices[1].loc, Some(Location { x: 3, y: 4 }));
        assert_eq!(packet.vertices[2].loc, Some(Location { x: 5, y: 6 }));
        assert_eq!(packet.vertices[3].loc, Some(Location { x: 7, y: 8 }));
        assert_eq!(
            packet.vertices[3].color,
            Some(Color {
                r: 0xA0,
                g: 0xB0,
                b: 0xC0,
            })
        );
    }

    #[test]
    fn builds_monochrome_line_packet() {
        let mut gpu = Gpu::default();
        let mut packet = LinePacket::init(0x4000_0000 | rgb(0x01, 0x02, 0x03));

        assert!(packet.needs_more());
        assert!(!packet.gouraud);
        assert_eq!(
            packet.color,
            Some(Color {
                r: 0x01,
                g: 0x02,
                b: 0x03,
            })
        );

        packet.push_cmd(loc(10, 20), &mut gpu);
        packet.push_cmd(loc(30, 40), &mut gpu);

        assert!(!packet.needs_more());
        assert_eq!(packet.vertices.len(), 2);
        assert_eq!(packet.vertices[0].loc, Some(Location { x: 10, y: 20 }));
        assert_eq!(packet.vertices[1].loc, Some(Location { x: 30, y: 40 }));
    }

    #[test]
    fn builds_polyline_until_terminator() {
        let mut gpu = Gpu::default();
        let mut packet = LinePacket::init(0x4800_0000 | rgb(0xAA, 0xBB, 0xCC));

        assert!(packet.needs_more());
        assert_eq!(packet.words_left, None);

        packet.push_cmd(loc(1, 1), &mut gpu);
        packet.push_cmd(loc(2, 2), &mut gpu);
        packet.push_cmd(loc(3, 3), &mut gpu);

        assert!(packet.needs_more());
        assert_eq!(packet.vertices.len(), 3);

        packet.push_cmd(0x5000_5000, &mut gpu);

        assert!(!packet.needs_more());
        assert_eq!(packet.vertices.len(), 3);
    }

    #[test]
    fn builds_variable_rectangle_packet() {
        let mut gpu = Gpu::default();
        let mut packet = RectPacket::init(0x6000_0000 | rgb(0x12, 0x34, 0x56));

        assert!(packet.needs_more());
        assert!(!packet.textured);
        assert_eq!(
            packet.color,
            Color {
                r: 0x12,
                g: 0x34,
                b: 0x56,
            }
        );
        assert_eq!(packet.size, None);

        packet.push_cmd(loc(-12, 34), &mut gpu);
        packet.push_cmd(20 | (30 << 16), &mut gpu);

        assert!(!packet.needs_more());
        assert_eq!(packet.loc, Some(Location { x: -12, y: 34 }));
        assert_eq!(packet.size, Some(Size { w: 20, h: 30 }));
    }

    #[test]
    fn builds_fixed_size_sprite_packet() {
        let mut gpu = Gpu::default();
        let mut packet = RectPacket::init(0x7800_0000 | rgb(0xFE, 0xDC, 0xBA));

        assert!(packet.needs_more());
        assert!(!packet.textured);
        assert_eq!(packet.size, Some(Size { w: 16, h: 16 }));

        packet.push_cmd(loc(100, 200), &mut gpu);

        assert!(!packet.needs_more());
        assert_eq!(packet.loc, Some(Location { x: 100, y: 200 }));
        assert_eq!(packet.size, Some(Size { w: 16, h: 16 }));
    }
}
