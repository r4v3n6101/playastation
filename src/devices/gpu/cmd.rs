use std::fmt::Debug;

use smallvec::SmallVec;

use super::types::{Clut, Color, Position, Size, UV};

const POLYGON_MAX_SIZE: usize = 4;
const POLYLINE_VERTICES_HARD_LIMIT: usize = 256;

#[derive(Debug)]
pub enum Packet {
    Polygon(PolygonPacket),
    Line(LinePacket),
    Rect(RectPacket),
}

#[derive(Debug)]
pub enum Remain {
    Count(usize),
    Terminator(u32),
}

#[derive(Debug)]
pub struct PolygonPacket {
    quad: bool,
    gouraud: bool,
    textured: bool,

    color: Option<Color>,
    vertices: SmallVec<[VertexPacket; POLYGON_MAX_SIZE]>,
    clut: Option<Clut>,
    tpage: Option<()>,
}

#[derive(Debug)]
pub struct LinePacket {
    gouraud: bool,

    color: Option<Color>,
    vertices: SmallVec<[VertexPacket; POLYLINE_VERTICES_HARD_LIMIT]>,
}

#[derive(Debug)]
pub struct RectPacket {
    textured: bool,

    color: Color,
    pos: Option<Position>,
    uv: Option<UV>,
    clut: Option<Clut>,
    size: Option<Size>,
}

#[derive(Debug, Default, Copy, Clone)]
pub struct VertexPacket {
    pub pos: Option<Position>,
    pub color: Option<Color>,
    pub uv: Option<UV>,
}

impl PolygonPacket {
    pub fn init(cmd: u32) -> (Self, Remain) {
        let op = (cmd >> 24) as u8;
        let quad = (op & 0x08) != 0;
        let gouraud = (op & 0x10) != 0;
        let textured = (op & 0x04) != 0;

        let mut vertices = SmallVec::new();
        let color = parse_color(cmd);
        let color = if !gouraud {
            Some(color)
        } else {
            vertices.push(VertexPacket {
                color: Some(color),
                ..Default::default()
            });

            None
        };

        // The first color is in initial word
        let more = Remain::Count(match (quad, gouraud, textured) {
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
        });

        (
            Self {
                quad,
                gouraud,
                textured,

                color,
                vertices,
                clut: None,
                tpage: None,
            },
            more,
        )
    }

    pub fn push_cmd(&mut self, cmd: u32) {
        loop {
            if let Some(last) = self.vertices.last_mut() {
                if self.gouraud
                    && let color @ None = &mut last.color
                {
                    color.replace(parse_color(cmd));
                    return;
                }
                if let pos @ None = &mut last.pos {
                    pos.replace(parse_pos(cmd));
                    return;
                }
                if self.textured
                    && let uv @ None = &mut last.uv
                {
                    // TODO : *uv = Some(());
                    return;
                }
            }
            self.vertices.push(VertexPacket::default());
        }
    }
}

impl LinePacket {
    pub fn init(cmd: u32) -> (Self, Remain) {
        const TERMINATOR_CMD: u32 = 0x5000_5000;

        let op = (cmd >> 24) as u8;
        let polyline = (op & 0x08) != 0;
        let gouraud = (op & 0x10) != 0;

        let mut vertices = SmallVec::new();
        let color = parse_color(cmd);
        let color = if !gouraud {
            Some(color)
        } else {
            vertices.push(VertexPacket {
                color: Some(color),
                ..Default::default()
            });
            None
        };

        // The first color is in initial word
        let more = match (polyline, gouraud) {
            // 2 vertices
            (false, false) => Remain::Count(2),
            // 2 vertices + color
            (false, true) => Remain::Count(3),
            // Until terminator
            (true, _) => Remain::Terminator(TERMINATOR_CMD),
        };

        (
            Self {
                gouraud,

                color,
                vertices,
            },
            more,
        )
    }

    pub fn push_cmd(&mut self, cmd: u32) {
        loop {
            if let Some(last) = self.vertices.last_mut() {
                if self.gouraud
                    && let color @ None = &mut last.color
                {
                    color.replace(parse_color(cmd));
                    return;
                }

                if let pos @ None = &mut last.pos {
                    pos.replace(parse_pos(cmd));
                    return;
                }
            }

            self.vertices.push(VertexPacket::default());
        }
    }
}

impl RectPacket {
    pub fn init(cmd: u32) -> (Self, Remain) {
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

        let more = Remain::Count(match (textured, size.is_some()) {
            // pos + size
            (false, false) => 2,
            // pos
            (false, true) => 1,

            // pos + uv/clut + size
            (true, false) => 3,
            // post + uv/clut
            (true, true) => 2,
        });

        (
            Self {
                textured,

                color,
                pos: None,
                uv: None,
                clut: None,
                size,
            },
            more,
        )
    }

    pub fn push_cmd(&mut self, cmd: u32) {
        if let pos @ None = &mut self.pos {
            pos.replace(parse_pos(cmd));
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
}

fn parse_color(cmd: u32) -> Color {
    Color {
        r: cmd as u8,
        g: (cmd >> 8) as u8,
        b: (cmd >> 16) as u8,
    }
}

fn parse_pos(cmd: u32) -> Position {
    Position {
        x: (cmd as u16).cast_signed(),
        y: ((cmd >> 16) as u16).cast_signed(),
    }
}

fn parse_size(cmd: u32) -> Size {
    Size {
        w: (cmd as u16),
        h: (cmd >> 16) as u16,
    }
}
