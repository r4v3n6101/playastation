use std::{
    fs,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};
use image::{ImageBuffer, Rgb};
use playastation::{Console, render::software::SoftwareRenderer};
use tracing::Level;

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Bios {
        path: PathBuf,
    },
    TestRom {
        path: PathBuf,
        #[arg(default_value_t = 0x8001_0000)]
        start_pc: u32,
    },
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .with_target(true)
        .with_level(true)
        .compact()
        .init();

    let args = Args::parse();

    let mut console = Console::default();
    let rom_filename = match args.command {
        Command::TestRom { path, start_pc } => {
            let prg = fs::read(&path).unwrap();
            for (i, byte) in prg.into_iter().enumerate() {
                console
                    .bus
                    .store(start_pc + i as u32, byte.to_le_bytes())
                    .unwrap();
            }
            console.executor.cpu.pc = start_pc;

            path.file_name().unwrap().to_os_string()
        }
        Command::Bios { path } => {
            let bios = fs::read(&path).unwrap();
            console.load_bios(&bios);

            path.file_name().unwrap().to_os_string()
        }
    };

    let mut renderer = SoftwareRenderer::default();
    renderer.set_screen_output(Box::new(move |buf, width, height| {
        let _ = dump_bgr555_texture_png(
            buf,
            width as u32,
            height as u32,
            format!("output/{:?}.png", rom_filename),
        );
    }));

    console.set_render(renderer);
    console.run();
}

fn dump_bgr555_texture_png(
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
