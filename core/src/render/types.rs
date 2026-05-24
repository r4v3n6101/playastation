use alloc::boxed::Box;

use smallvec::SmallVec;

/// Maximum polygon is quad, but what if greater?
pub const POLYGON_STACK_LIMIT: usize = 4;
/// Points for polyline that will be stored on a stack. If more then heap alloc.
pub const POLYLINE_STACK_LIMIT: usize = 10;

pub const VRAM_WIDTH: usize = 1024;
pub const VRAM_HEIGHT: usize = 512;

/// VRAM is 2d texture actually.
pub type Vram = Box<[u16]>;

#[derive(Debug, Copy, Clone)]
pub struct RenderState {}

#[derive(Debug, Clone)]
pub struct Polygon {
    pub vertices: SmallVec<[Vertex; POLYGON_STACK_LIMIT]>,
    pub flat_color: Option<Color>,
    pub clut: Option<Position>,
    pub tpage: Option<Position>,
}

#[derive(Debug, Clone)]
pub struct Polyline {
    pub vertices: SmallVec<[Vertex; POLYLINE_STACK_LIMIT]>,
    pub flat_color: Option<Color>,
}

#[derive(Debug, Copy, Clone)]
pub struct Rect {
    pub location: Location,
    pub size: Size,
    pub flat_color: Color,
    pub texcoords: Option<UV>,
    pub clut: Option<Position>,
}

#[derive(Debug, Copy, Clone)]
pub struct Vertex {
    pub location: Location,
    pub color: Option<Color>,
    pub texcords: Option<UV>,
}

#[derive(Debug, Copy, Clone)]
pub struct TextureWindow {
    pub mask_x: u8,
    pub mask_y: u8,
    pub offset_x: u8,
    pub offset_y: u8,
}

/// Position somewhere at space.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Location {
    pub x: i16,
    pub y: i16,
}

/// Position in VRAM space. Must not exceed VRAM size.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Position {
    pub x: u16,
    pub y: u16,
}

/// Size of rectangle.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Size {
    pub w: u16,
    pub h: u16,
}

/// RGB color (24-bit).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Texture coordinates inside of 256x256 texture page.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct UV {
    pub u: u8,
    pub v: u8,
}
