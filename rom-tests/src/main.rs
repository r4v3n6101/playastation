use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};

use clap::Parser;
use image::{ImageBuffer, Rgb};
use minifb::{Window, WindowOptions};
use playastation::{
    formats::BoxedExeFile, interconnect::Bus, render::software::SoftwareRenderer, run::Executor,
};
use tracing::Level;

const LOOPS: usize = 400_000_000;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    bios: PathBuf,
    #[arg(long)]
    rom: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    window: bool,
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(true)
        .with_level(true)
        .compact()
        .init();

    let args = Args::parse();

    let (tx, rx) = mpsc::channel();

    let emu_thread = thread::spawn(move || {
        let mut bus = Bus::default();
        let mut executor = Executor::default();

        let bios = fs::read(&args.bios).unwrap();
        bus.bios.copy_from_slice(&bios);

        if let Some(rom_path) = &args.rom {
            let rom = fs::read(rom_path).unwrap().into_boxed_slice();
            executor.pending_exe = Some(BoxedExeFile::new(rom));
        }

        let mut renderer = SoftwareRenderer::default();
        if args.window {
            renderer.screen_fill = Box::new(move |buf, width, height| {
                tx.send((buf.to_vec(), width, height)).unwrap();
            });
        } else {
            let name = args
                .rom
                .and_then(|path| path.file_name().map(OsStr::to_os_string))
                .unwrap_or_else(|| OsString::from("bios"));
            renderer.screen_fill = Box::new(move |buf, width, height| {
                let _ = dump_bgr555_image(
                    buf,
                    width as u32,
                    height as u32,
                    format!("output/{}.bmp", name.display()),
                );
            });
        }
        bus.gpu.renderer = Box::new(renderer);
        for _ in 0..LOOPS {
            executor.run(&mut bus);
        }
    });

    if args.window {
        let mut window = Window::new("viewport", 800, 600, WindowOptions::default()).unwrap();
        while let Ok((buf, width, height)) = rx.recv() {
            let buf32 = buf.iter().copied().map(bgr555_to_0rgb).collect::<Vec<_>>();
            window.update_with_buffer(&buf32, width, height).unwrap();

            // 60 FPS
            thread::sleep(Duration::from_secs(1) / 60);
        }
    } else {
        emu_thread.join().unwrap();
    }
}

fn dump_bgr555_image(
    data: &[u16],
    width: u32,
    height: u32,
    path: impl AsRef<Path>,
) -> image::ImageResult<()> {
    assert_eq!(data.len(), width as usize * height as usize);

    let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let idx = y as usize * width as usize + x as usize;
            let rgb = bgr555_to_rgb888(data[idx]);
            img.put_pixel(x, y, Rgb(rgb));
        }
    }

    img.save(path)
}

fn bgr555_to_rgb888(pixel: u16) -> [u8; 3] {
    let r5 = (pixel & 0x1f) as u8;
    let g5 = ((pixel >> 5) & 0x1f) as u8;
    let b5 = ((pixel >> 10) & 0x1f) as u8;
    let r8 = (r5 << 3) | (r5 >> 2);
    let g8 = (g5 << 3) | (g5 >> 2);
    let b8 = (b5 << 3) | (b5 >> 2);

    [r8, g8, b8]
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
