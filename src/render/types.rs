use smallvec::SmallVec;

/// Maximum polygon is quad, but what if greater?
pub const POLYGON_STACK_LIMIT: usize = 4;
/// Points for polyline that will be stored on a stack. If more then heap alloc.
pub const POLYLINE_STACK_LIMIT: usize = 10;

#[derive(Debug, Clone)]
pub struct Polygon {
    pub vertices: SmallVec<[Vertex; POLYGON_STACK_LIMIT]>,
    pub flat_color: Option<Color>,
    pub clut: Option<Clut>,
    pub tpage: Option<()>,
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
    pub clut: Option<Clut>,
}

#[derive(Debug, Copy, Clone)]
pub struct Vertex {
    pub location: Location,
    pub color: Option<Color>,
    pub texcords: Option<UV>,
}

/// Relative position in draw space (i.e. space in VRAM with offset).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Location {
    pub x: i16,
    pub y: i16,
}

/// Absolute position in VRAM space.
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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct UV {
    pub u: u8,
    pub v: u8,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Clut {
    /// X coordinate in VRAM, already multiplied by 16.
    pub x: u16,
    pub y: u16,
}
