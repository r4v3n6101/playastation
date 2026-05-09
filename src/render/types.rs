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
