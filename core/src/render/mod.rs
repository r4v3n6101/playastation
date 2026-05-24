pub mod noop;
pub mod software;
pub mod types;

pub trait Renderer: 'static {
    /// Gather inner fields into [`types::RenderState`].
    fn state(&self) -> types::RenderState;

    /// Draw frame somehow, triggerred on every vblank.
    fn draw_frame(&mut self);

    /// Change inner render state.
    fn set_parameter(&mut self, param: types::EnvParameter);

    /// Draw a polygon.
    fn draw_polygon(&mut self, polygon: types::Polygon);

    /// Draw a polyline - line with N points.
    fn draw_polyline(&mut self, polyline: types::Polyline);

    /// Draw a rectangle (actually a polygon, but in another format).
    fn draw_rect(&mut self, rect: types::Rect);

    /// Fill VRAM area with color.
    fn fill_vram_area(&mut self, pos: types::Position, size: types::Size, color: types::Color);

    /// Blit VRAM area to the local storage. This is done just before VRAM => CPU transfer.
    fn prepare_vram_for_read(&mut self, pos: types::Position, size: types::Size);

    /// Read a pixel from the VRAM and move pointer forward to the next pixel (if any left).
    fn pop_pixel(&mut self) -> Option<u16>;

    /// Prepare inner state to gather pixels into the VRAM.
    fn prepare_vram_for_write(&mut self, pos: types::Position, size: types::Size);

    /// Push a pixel into the VRAM and increment pointer to the next pixel to be written.
    fn push_pixel(&mut self, pixel: u16);

    /// Copy VRAM area into VRAM.
    fn mirror_vram_area(&mut self, src: types::Position, dest: types::Position, size: types::Size);

    /// Reset inner state like push/pop pointers, etc.
    fn reset(&mut self);
}
