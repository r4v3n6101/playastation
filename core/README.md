# PlayaStation

An experimental PlayStation (PS1/PSX) emulation core written in Rust.

The crate is `no_std` with `alloc` and does not provide a windowing, audio, filesystem, or event-loop abstraction. These are left to the host application.

## Status

Currently implemented:

- MIPS R3000A CPU interpreter with a decoded-block cache
- Geometry Transformation Engine (GTE/COP2)
- GPU command processing and software rendering
- Interrupts and timers
- DMA
- CD-ROM emulation (partial)
- Digital controller input

The emulator is still experimental and game compatibility is limited.
I've tested "Silent Hill" and it is playable regarding missing FMV-s and no sound at all.

## Usage

Nightly Rust is currently required because `smallbox` uses its `coerce` feature.

```toml
[dependencies]
playastation = "0.1.0"
```

A host using PlayaStation is responsible for providing:

- a PlayStation BIOS image
- disc access
- controller input
- video presentation
- wall-clock pacing (so host CPU moves with guest CPU speed)

The core itself does not access the filesystem or sleep to maintain real-time emulation.

Disc images are provided by implementing `formats::disk::Disc`. Controller input can be provided through `DigitalController`, and rendered VRAM is exposed through the configured `Renderer`.

A minimal host can look like this:

```rust
use playastation::{
    devices::joy::{
        controller::{Button, DigitalController},
        Slot,
    },
    formats::disk::{Disc, RawSector},
    interconnect::BIOS_SIZE,
    render::software::SoftwareRenderer,
    run::Console,
};

struct MyDisc;

impl Disc for MyDisc {
    fn read_sector(&mut self, lba: usize) -> Option<RawSector> {
        // Read sector `lba` from your disc image.
        todo!()
    }

    fn sector_count(&self) -> usize {
        todo!()
    }
}

fn main() {
    let bios: Vec<u8> = todo!("load a 512 KiB PS1 BIOS");

    assert_eq!(bios.len(), BIOS_SIZE);

    let mut console = Console::default();

    console.bus.bios.copy_from_slice(&bios);
    console.bus.cdrom.disc = Some(Box::new(MyDisc));
    console.bus.gpu.renderer = Box::new(SoftwareRenderer::default());

    console.bus.joy_bus.insert_dev(
        Slot::Controller1,
        Box::new(DigitalController::with_poll_buttons(Box::new(|| {
            Button::empty()
        }))),
    );

    loop {
        let elapsed_cycles = console.step();

        // Use `elapsed_cycles` for wall-clock pacing.

        if console.bus.gpu.take_frame_ready() {
            // Present the current framebuffer.
        }
    }
}
```

`Console::step()` advances the emulator and returns the number of elapsed CPU cycles. It does not correspond to one instruction or one frame.

`Disc::read_sector()` operates on `RawSector`, which contains 2340 bytes: a raw 2352-byte CD sector without its 12-byte synchronization prefix.

For more specialized integrations, custom rendering can be implemented through `render::Renderer`.

## Limitations

Several parts of the PlayStation hardware are still missing or incomplete:

- SPU audio synthesis and CD audio playback
- MDEC video decoding
- memory cards
- complete CD-ROM command and track handling
- some CPU, DMA, and device access paths

BIOS and game data are not included.

## License

[WTFPL](https://github.com/r4v3n6101/playastation/blob/master/LICENSE).
