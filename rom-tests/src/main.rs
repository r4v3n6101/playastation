use std::{
    fs,
    num::NonZeroU32,
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use clap::Parser;
use crossbeam_utils::atomic::AtomicCell;
use playastation::{
    VRAM_HEIGHT, VRAM_WIDTH,
    devices::joy::{
        Slot,
        controller::{Button, DigitalController},
    },
    formats::{
        disk::{Disc, RawSector},
        psexe::BoxedExeFile,
    },
    render::software::SoftwareRenderer,
    run::Executor,
};
use softbuffer::{Context, Surface};
use tracing::Level;
use triple_buffer::Output;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowAttributes, WindowId},
};

const WIDTH: u32 = 1200;
const HEIGHT: u32 = 800;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    bios: PathBuf,
    #[arg(long)]
    rom: Option<PathBuf>,
    #[arg(long)]
    bin: Option<PathBuf>,
}

struct App {
    window: Option<Rc<Window>>,
    surface: Option<Surface<Rc<Window>, Rc<Window>>>,
    button_state: Arc<AtomicCell<Button>>,
    image_buf: Output<Vec<u16>>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = WindowAttributes::default()
            .with_title("viewport")
            .with_inner_size(PhysicalSize::new(WIDTH, HEIGHT));

        let window = Rc::new(event_loop.create_window(attrs).unwrap());
        let context = Context::new(window.clone()).unwrap();

        let mut surface = Surface::new(&context, window.clone()).unwrap();
        surface
            .resize(
                NonZeroU32::new(VRAM_WIDTH as _).unwrap(),
                NonZeroU32::new(VRAM_HEIGHT as _).unwrap(),
            )
            .unwrap();

        self.window = Some(window);
        self.surface = Some(surface);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let fun = |old: &mut Button, btn| match event.state {
                    ElementState::Pressed => old.insert(btn),
                    ElementState::Released => old.remove(btn),
                };
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::KeyW) => {
                        let _ = self.button_state.fetch_update(|mut old| {
                            fun(&mut old, Button::UP);
                            Some(old)
                        });
                    }
                    PhysicalKey::Code(KeyCode::KeyA) => {
                        let _ = self.button_state.fetch_update(|mut old| {
                            fun(&mut old, Button::LEFT);
                            Some(old)
                        });
                    }
                    PhysicalKey::Code(KeyCode::KeyS) => {
                        let _ = self.button_state.fetch_update(|mut old| {
                            fun(&mut old, Button::DOWN);
                            Some(old)
                        });
                    }
                    PhysicalKey::Code(KeyCode::KeyD) => {
                        let _ = self.button_state.fetch_update(|mut old| {
                            fun(&mut old, Button::RIGHT);
                            Some(old)
                        });
                    }
                    PhysicalKey::Code(KeyCode::KeyZ) => {
                        let _ = self.button_state.fetch_update(|mut old| {
                            fun(&mut old, Button::SQUARE);
                            Some(old)
                        });
                    }
                    PhysicalKey::Code(KeyCode::KeyX) => {
                        let _ = self.button_state.fetch_update(|mut old| {
                            fun(&mut old, Button::CROSS);
                            Some(old)
                        });
                    }
                    PhysicalKey::Code(KeyCode::KeyC) => {
                        let _ = self.button_state.fetch_update(|mut old| {
                            fun(&mut old, Button::CIRCLE);
                            Some(old)
                        });
                    }
                    PhysicalKey::Code(KeyCode::KeyV) => {
                        let _ = self.button_state.fetch_update(|mut old| {
                            fun(&mut old, Button::TRIANGLE);
                            Some(old)
                        });
                    }
                    PhysicalKey::Code(KeyCode::Enter) => {
                        let _ = self.button_state.fetch_update(|mut old| {
                            fun(&mut old, Button::START);
                            Some(old)
                        });
                    }
                    PhysicalKey::Code(KeyCode::Escape) => {
                        event_loop.exit();
                    }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                let Some(surface) = self.surface.as_mut() else {
                    return;
                };

                let buf = self.image_buf.read();
                let buf32 = buf.iter().copied().map(bgr555_to_0rgb).collect::<Vec<_>>();

                surface
                    .resize(
                        NonZeroU32::new(VRAM_WIDTH as _).unwrap(),
                        NonZeroU32::new(VRAM_HEIGHT as _).unwrap(),
                    )
                    .unwrap();

                let mut buffer = surface.buffer_mut().unwrap();
                buffer.copy_from_slice(&buf32);
                buffer.present().unwrap();

                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(true)
        .with_level(true)
        .compact()
        .init();

    let args = Args::parse();

    let (mut img_tx, img_rx) = triple_buffer::triple_buffer(&vec![0; VRAM_WIDTH * VRAM_HEIGHT]);
    let button_state = Arc::new(AtomicCell::new(Button::empty()));

    let mut app = App {
        window: None,
        surface: None,
        button_state: Arc::clone(&button_state),
        image_buf: img_rx,
    };
    thread::spawn(move || {
        let mut executor = Executor::default();

        let bios = fs::read(&args.bios).unwrap();
        executor.bus.bios.copy_from_slice(&bios);

        if let Some(rom_path) = &args.rom {
            let rom = fs::read(rom_path).unwrap().into_boxed_slice();
            executor.pending_exe = Some(BoxedExeFile::new(rom));
        }

        if let Some(bin_path) = &args.bin {
            let bin = fs::read(bin_path).unwrap();
            executor.bus.cdrom.disc = Some(Box::new(BinFile { data: bin }));
        }

        executor.bus.gpu.renderer = Box::new(SoftwareRenderer::default());
        executor.bus.joy_bus.insert_dev(
            Slot::Controller1,
            Box::new(DigitalController::with_poll_buttons(Box::new(move || {
                button_state.load()
            }))),
        );

        let mut last_frame = Instant::now();
        loop {
            executor.run();

            if last_frame.elapsed() > Duration::from_secs(1) / 60 {
                img_tx
                    .input_buffer_publisher()
                    .copy_from_slice(executor.bus.gpu.renderer.framebuffer());
                last_frame = Instant::now();
            }
        }
    });

    let event_loop = EventLoop::new().unwrap();
    event_loop.run_app(&mut app).unwrap();
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

struct BinFile {
    data: Vec<u8>,
}

impl Disc for BinFile {
    fn read_sector(&mut self, lba: usize) -> Option<RawSector> {
        let chunk = self.data.get(lba * 2352 + 12..)?.get(..2340)?;

        Some(chunk.try_into().unwrap())
    }
}
