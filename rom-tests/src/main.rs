use std::{fs, path::PathBuf, sync::Arc, thread};

use clap::Parser;
use crossbeam_utils::atomic::AtomicCell;
use minifb::{Key, Window, WindowOptions};
use playastation::{
    devices::joy::{
        Slot,
        controller::{Button, DigitalController},
    },
    formats::BoxedExeFile,
    interconnect::Bus,
    render::software::SoftwareRenderer,
    run::Executor,
};
use tracing::Level;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    bios: PathBuf,
    #[arg(long)]
    rom: Option<PathBuf>,
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(true)
        .with_level(true)
        .compact()
        .init();

    let args = Args::parse();

    let (mut img_tx, mut img_rx) = triple_buffer::triple_buffer(&(Vec::new(), 0, 0));
    let button_state = Arc::new(AtomicCell::new(Button::empty()));

    let pressed = Arc::clone(&button_state);
    thread::spawn(move || {
        let mut bus = Bus::default();
        let mut executor = Executor::default();

        let bios = fs::read(&args.bios).unwrap();
        bus.bios.copy_from_slice(&bios);

        if let Some(rom_path) = &args.rom {
            let rom = fs::read(rom_path).unwrap().into_boxed_slice();
            executor.pending_exe = Some(BoxedExeFile::new(rom));
        }

        bus.gpu.renderer = Box::new(SoftwareRenderer::with_screen_fill(Box::new(
            move |buf, width, height| {
                img_tx.write((buf.to_vec(), width, height));
            },
        )));

        bus.joy_bus.insert_dev(
            Slot::Controller1,
            Box::new(DigitalController::with_poll_buttons(Box::new(move || {
                pressed.load()
            }))),
        );

        loop {
            executor.run(&mut bus);
        }
    });

    let mut window = Window::new("viewport", 800, 600, WindowOptions::default()).unwrap();
    while window.is_open() {
        let mut pressed = Button::empty();
        if window.is_key_down(Key::W) {
            pressed.insert(Button::Up);
        }
        if window.is_key_down(Key::S) {
            pressed.insert(Button::Down);
        }
        if window.is_key_down(Key::X) {
            pressed.insert(Button::Cross);
        }
        if window.is_key_down(Key::Enter) {
            pressed.insert(Button::Start);
        }
        button_state.store(pressed);

        let (buf, width, height) = img_rx.read();
        let buf32 = buf.iter().copied().map(bgr555_to_0rgb).collect::<Vec<_>>();
        window.update_with_buffer(&buf32, *width, *height).unwrap();
    }
}

fn bgr555_to_0rgb(color: u16) -> u32 {
    let r5 = (color & 0x001f) as u32;
    let g5 = ((color >> 5) & 0x001f) as u32;
    let b5 = ((color >> 10) & 0x001f) as u32;

    // Expand 5-bit channel to 8-bit:
    // abcde -> abcdeabc
    let r8 = (r5 << 3) | (r5 >> 2);
    let g8 = (g5 << 3) | (g5 >> 2);
    let b8 = (b5 << 3) | (b5 >> 2);

    (r8 << 16) | (g8 << 8) | b8
}
