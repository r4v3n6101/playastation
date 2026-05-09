use playastation::Console;
use tracing::Level;

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::TRACE)
        .with_target(true)
        .with_level(true)
        .compact()
        .init();

    let mut console = Console::default();
    console.load_bios(include_bytes!(env!("PSX_BIOS")));

    loop {
        console.run();
    }
}
