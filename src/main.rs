use playastation::{Console, render::software::SoftwareRenderer};
use tracing::Level;

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::TRACE)
        .with_target(true)
        .with_level(true)
        .compact()
        .init();

    Console::default()
        .load_bios(include_bytes!(env!("PSX_BIOS")))
        .set_render(SoftwareRenderer::default())
        .run();
}
