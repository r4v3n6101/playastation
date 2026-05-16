pub mod noop;
pub mod software;
pub mod types;

pub trait Renderer: 'static {
    /// Gather inner fields into [`types::RenderState`].
    fn state(&self) -> types::RenderState;

    /// Clip point (top-left) of bounding box of draw window.
    fn set_draw_area_top_left(&mut self, pos: types::Position);

    /// Clip point (bottom-right) of bounding box of draw window.
    fn set_draw_area_bottom_right(&mut self, pos: types::Position);

    /// Draw offset for converting primitives space into screen space.
    /// Though, it is not VRAM and should be clipped.
    fn set_draw_offset(&mut self, loc: types::Location);

    /// Draw a polygon.
    fn draw_polygon(&mut self, polygon: types::Polygon);

    /// Draw a polyline - line with N points.
    fn draw_polyline(&mut self, polyline: types::Polyline);

    /// Draw a rectangle (actually a polygon, but in another format).
    fn draw_rect(&mut self, rect: types::Rect);

    /// Fill VRAM area with color.
    fn fill_vram_area(&mut self, pos: types::Position, size: types::Size, color: types::Color);

    /// Blit VRAM area to the local storage. This is done just before VRAM => CPU transfer.
    fn download_vram_area_to_local(&mut self, pos: types::Position, size: types::Size);

    /// Read a pixel from VRAM snapshot taken via [`Self::download_vram_area_to_local`]
    /// and move pointer forward to the next pixel (if any have left in the area of snapshot).
    fn pop_pixel(&mut self) -> Option<u16>;

    /// Prepare an inner state to gather pixels for the future commit into the VRAM.
    fn prepare_local_vram_to_upload(&mut self, pos: types::Position, size: types::Size);

    /// Push a pixel into the VRAM snapshot and increment pointer to the next pixel to be written.
    fn push_pixel(&mut self, pixel: u16);

    /// Commit gathered pixels into VRAM ending upload started after [`Self::prepare_local_vram_to_upload`].
    fn upload_local_vram_area(&mut self);

    /// Copy VRAM area into VRAM.
    fn mirror_vram_area(&mut self, src: types::Position, dest: types::Position, size: types::Size);

    /// Reset inner state like push/pop pointers, etc.
    fn reset(&mut self);
}
