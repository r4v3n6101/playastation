use alloc::boxed::Box;

use modular_bitfield::prelude::*;
use smallvec::SmallVec;

/// Maximum polygon is quad, but what if greater?
pub const POLYGON_STACK_LIMIT: usize = 4;
/// Points for polyline that will be stored on a stack. If more then heap alloc.
pub const POLYLINE_STACK_LIMIT: usize = 10;

pub const VRAM_WIDTH: usize = 1024;
pub const VRAM_HEIGHT: usize = 512;

/// VRAM is 2d texture actually.
pub type Vram = Box<[u16]>;

#[derive(Debug, Clone)]
pub struct RenderState {
    pub draw_mode: DrawMode,
    pub mask_bit_setting: MaskBitSetting,
    pub vram_read_active: bool,
}

#[derive(Debug, Clone)]
pub enum EnvParameter {
    DrawMode(DrawMode),
    TextureWindow(TextureWindow),
    DrawAreaTopLeft(Position),
    DrawAreaBottomRight(Position),
    DrawOffset(Location),
    MaskBitSetting(MaskBitSetting),
}

#[bitfield(bits = 14)]
#[derive(Debug, Copy, Clone)]
pub struct DrawMode {
    pub texture_page_x_base: B4,
    pub texture_page_y_base: B1,
    pub semi_transparency: SemiTransparency,
    pub texture_depth: TextureDepth,
    pub dither_24_to_15: bool,
    pub draw_to_display_area: bool,
    pub texture_disable: bool,
    pub texture_rectangle_x_flip: bool,
    pub texture_rectangle_y_flip: bool,
}

#[bitfield(bits = 20)]
#[derive(Debug, Copy, Clone)]
pub struct TextureWindow {
    pub mask_x: B5,
    pub mask_y: B5,
    pub offset_x: B5,
    pub offset_y: B5,
}

#[bitfield(bits = 2)]
#[derive(Debug, Copy, Clone)]
pub struct MaskBitSetting {
    pub set_mask_while_drawing: bool,
    pub draw_to_masked_pixels: bool,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 2]
pub enum SemiTransparency {
    Average = 0,
    Add = 1,
    Subtract = 2,
    AddQuarter = 3,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 2]
pub enum TextureDepth {
    Bpp4 = 0,
    Bpp8 = 1,
    Bpp15 = 2,
    Reserved = 3,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 2]
pub enum HorizontalResolution {
    H256 = 0,
    H320 = 1,
    H512 = 2,
    H640 = 3,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 1]
pub enum VerticalResolution {
    V240 = 0,
    V480 = 1,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 1]
pub enum VideoMode {
    Ntsc = 0,
    Pal = 1,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 1]
pub enum DisplayDepth {
    Bpp15 = 0,
    Bpp24 = 1,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 2]
pub enum DmaDirection {
    Off = 0,
    Fifo = 1,
    CpuToGp0 = 2,
    VramToCpu = 3,
}

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
