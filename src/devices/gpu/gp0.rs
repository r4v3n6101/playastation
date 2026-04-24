use std::fmt;

use smallbox::{SmallBox, space::S32};
use smallvec::SmallVec;
use strum::FromRepr;

use super::{
    VRAM_HEIGHT, VRAM_WIDTH, Vram,
    types::{Clut, Color, Location, Position, Size, UV},
};

/// Maximum polygon is quad, but what if greater?
const POLYGON_STACK_LIMIT: usize = 4;
/// Points for polyline that will be stored on a stack. If more then heap alloc.
const POLYLINE_STACK_LIMIT: usize = 10;

#[derive(Debug)]
pub struct CmdBuf {
    packet: SmallBox<dyn Packet, S32>,
}

#[derive(FromRepr, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Gp0OpcodeGroup {
    Misc = 0x0,
    Polygon = 0x1,
    Line = 0x2,
    Rect = 0x3,
    Vram2Vram = 0x4,
    Cpu2Vram = 0x5,
    Vram2Cpu = 0x6,
    Env = 0x7,
}

/// [`Default`] state is like after NOP command.
impl Default for CmdBuf {
    fn default() -> Self {
        Self {
            packet: SmallBox::new(()),
        }
    }
}

impl CmdBuf {
    pub fn clear(&mut self) {
        self.packet = SmallBox::new(());
    }

    pub fn push_cmd(&mut self, cmd: u32, vram: &mut Vram) {
        if self.packet.needs_more() {
            self.packet.push_cmd(cmd, vram);
        } else {
            let opcode = (cmd >> 24) as u8;
            let group = (cmd >> 29) as u8;
            let group = Gp0OpcodeGroup::from_repr(group);
            println!("{group:?} = {cmd:#x}");
            match (group, opcode) {
                (_, 0x00 | 0x03..=0x1E) => {
                    // NOP
                    self.packet = SmallBox::new(());
                }
                (_, 0x01) => {
                    // Clear CLUT AFAIK
                    self.packet = SmallBox::new(());
                }
                (_, 0x02) => {
                    println!("fill vram");
                    // FillVram
                }
                (Some(Gp0OpcodeGroup::Polygon), _) => {
                    self.packet = SmallBox::new(PolygonPacket::init(cmd));
                }
                (Some(Gp0OpcodeGroup::Line), _) => {
                    self.packet = SmallBox::new(LinePacket::init(cmd));
                }
                (Some(Gp0OpcodeGroup::Rect), _) => {
                    self.packet = SmallBox::new(RectPacket::init(cmd));
                }
                (Some(Gp0OpcodeGroup::Vram2Vram), _) => {}
                (Some(Gp0OpcodeGroup::Cpu2Vram), _) => {
                    self.packet = SmallBox::new(Cpu2VramPacket::init(cmd));
                }
                (Some(Gp0OpcodeGroup::Vram2Cpu), _) => {}
                _ => {}
            }
        }

        if !self.packet.needs_more() {
            // TODO : commit
            self.clear();
        }
    }
}

trait Packet: fmt::Debug {
    fn init(cmd: u32) -> Self
    where
        Self: Sized;

    fn push_cmd(&mut self, cmd: u32, vram: &mut Vram);

    fn needs_more(&self) -> bool;
}

#[derive(Debug)]
struct PolygonPacket {
    gouraud: bool,
    textured: bool,

    color: Option<Color>,
    vertices: SmallVec<[VertexBuilder; POLYGON_STACK_LIMIT]>,
    clut: Option<Clut>,
    tpage: Option<()>,

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

    color: Color,
    loc: Option<Location>,
    uv: Option<UV>,
    clut: Option<Clut>,
    size: Option<Size>,

    words_left: usize,
}

#[derive(Debug)]
struct Cpu2VramPacket {
    pos: Option<Position>,
    size: Option<Size>,

    pixels_written: u32,
}

#[derive(Debug, Default, Copy, Clone)]
struct VertexBuilder {
    loc: Option<Location>,
    color: Option<Color>,
    uv: Option<UV>,
}

impl Packet for () {
    fn init(_: u32) -> Self
    where
        Self: Sized,
    {
    }

    fn push_cmd(&mut self, _: u32, _: &mut Vram) {}

    fn needs_more(&self) -> bool {
        false
    }
}

impl Packet for PolygonPacket {
    fn init(cmd: u32) -> Self
    where
        Self: Sized,
    {
        let op = (cmd >> 24) as u8;
        let quad = (op & 0x08) != 0;
        let gouraud = (op & 0x10) != 0;
        let textured = (op & 0x04) != 0;

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

            color,
            vertices,
            clut: None,
            tpage: None,

            words_left,
        }
    }

    fn push_cmd(&mut self, cmd: u32, _: &mut Vram) {
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
                    // TODO : *uv = Some(());
                    return;
                }
            }
            self.vertices.push(VertexBuilder::default());
        }
    }

    fn needs_more(&self) -> bool {
        self.words_left > 0
    }
}

impl Packet for LinePacket {
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

    fn push_cmd(&mut self, cmd: u32, _: &mut Vram) {
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
}

impl Packet for RectPacket {
    fn init(cmd: u32) -> Self
    where
        Self: Sized,
    {
        let op = (cmd >> 24) as u8;
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

            color,
            loc: None,
            uv: None,
            clut: None,
            size,

            words_left,
        }
    }

    fn push_cmd(&mut self, cmd: u32, _: &mut Vram) {
        self.words_left -= 1;

        if let loc @ None = &mut self.loc {
            loc.replace(parse_loc(cmd));
            return;
        }
        if self.textured
            && let uv @ None = &mut self.uv
        {
            // TODO : *uv = Some(());
            // TODO : self.clut = Some(());
            return;
        }

        self.size.replace(parse_size(cmd));
    }

    fn needs_more(&self) -> bool {
        self.words_left > 0
    }
}

impl Packet for Cpu2VramPacket {
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

    fn push_cmd(&mut self, cmd: u32, vram: &mut Vram) {
        if let pos @ None = &mut self.pos {
            pos.replace(parse_pos(cmd));
            return;
        }
        match &mut self.size {
            size @ None => {
                size.replace(parse_size(cmd));
            }
            Some(size) => {
                debug_assert!(self.pixels_written <= u32::from(size.w) * u32::from(size.h));

                for pixel in [cmd as u16, (cmd >> 16) as u16] {
                    let (x, y) = (
                        self.pixels_written % u32::from(size.w),
                        self.pixels_written / u32::from(size.w),
                    );
                    let x = (self.pos.unwrap().x + x as u16) as usize;
                    let y = (self.pos.unwrap().y + y as u16) as usize;

                    if x <= VRAM_WIDTH && y <= VRAM_HEIGHT {
                        vram[y][x] = pixel;
                    }

                    self.pixels_written = self.pixels_written.wrapping_add(1);
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
        x: (cmd as u16).cast_signed(),
        y: ((cmd >> 16) as u16).cast_signed(),
    }
}

fn parse_pos(cmd: u32) -> Position {
    Position {
        x: (cmd as u16),
        y: (cmd >> 16) as u16,
    }
}

fn parse_size(cmd: u32) -> Size {
    Size {
        w: (cmd as u16),
        h: (cmd >> 16) as u16,
    }
}
