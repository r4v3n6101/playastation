use std::{env, fs, path::Path};

use image::{ImageBuffer, Rgb};
use playastation::{Console, render::software::SoftwareRenderer};
use tracing::Level;

const ROM_OFFSET: u32 = 0x80010000;

enum Rom {
    Bios,
    Custom(String),
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .with_target(true)
        .with_level(true)
        .compact()
        .init();

    let mut args = env::args();
    args.next();
    let (filename, rom) = match args.next() {
        Some(path) => (
            Path::new(&path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string(),
            Rom::Custom(path),
        ),
        None => ("bios".to_string(), Rom::Bios),
    };

    let mut console = Console::default();

    match rom {
        Rom::Bios => {
            let bios = fs::read(env::var("PSX_BIOS").unwrap()).unwrap();
            console.load_bios(&bios);
        }
        Rom::Custom(path) => {
            let prg = fs::read(path).unwrap();
            for (i, byte) in prg.into_iter().enumerate() {
                console
                    .bus
                    .store(ROM_OFFSET + i as u32, byte.to_le_bytes())
                    .unwrap();
            }
            console.executor.cpu.pc = ROM_OFFSET;
        }
    }

    let mut renderer = SoftwareRenderer::default();
    renderer.set_screen_output(Box::new(move |buf, width, height| {
        let _ = dump_bgr555_texture_png(
            buf,
            width as u32,
            height as u32,
            format!("output/{filename}.png"),
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
