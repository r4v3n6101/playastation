use std::{sync::mpsc, thread};

use image::{ImageBuffer, Rgb};
use playastation::{Console, render::software::SoftwareRenderer};
use tracing::Level;

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .with_target(true)
        .with_level(true)
        .compact()
        .init();

    let (tx, rx) = mpsc::channel();

    thread::spawn(|| {
        let mut renderer = SoftwareRenderer::default();
        renderer.set_screen_output(Box::new(move |buf, width, height| {
            let _ = tx.send((buf.to_vec(), width, height));
        }));
        Console::default()
            .load_bios(include_bytes!(env!("PSX_BIOS")))
            .set_render(renderer)
            .run();
    });

    while let Ok((buf, width, height)) = rx.recv() {
        let _ = dump_bgr555_texture_png(&buf, width as u32, height as u32, "vram.png");
    }
}

fn dump_bgr555_texture_png(
    data: &[u16],
    width: u32,
    height: u32,
    path: impl AsRef<std::path::Path>,
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
