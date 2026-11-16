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

Disc images are provided by implementing `disc::Disc`. Controller input can be provided through `DigitalController`, and rendered VRAM is exposed through the configured `Renderer`.

A minimal host can look like this:

```rust
use playastation::{
    devices::joy::{
        controller::{Button, DigitalController},
        Slot,
    },
    disc::{Disc, RawSector},
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

`Disc::read_sector()` operates on `RawSector`, a complete 2352-byte CD sector, including its 12-byte synchronization prefix. `Disc::tracks()` describes the disc TOC: each track's mode, INDEX 01 LBA, and preceding INDEX 00 pregap. It defaults to one Mode 2 data track for plain BIN images.

`formats::cue::CueSheet::parse()` parses CUE text without filesystem access or `std`. It supports BINARY files with AUDIO, MODE1/2352, and MODE2/2352 tracks, shared or separate BIN files, INDEX 00/01, PREGAP, and POSTGAP. It returns file-relative indexes. `disc::DiscImage::from_cue()` validates the BIN sizes and track boundaries and resolves these indexes into disc LBAs. Sources implement `disc::DiscSource` to expose their byte length and read raw sectors at byte offsets; filesystem access stays in the frontend. Track numbers must run consecutively from 01, with at most 99 tracks.

The desktop frontend opens CUE sheets with `--disc /path/to/game.cue` (`--cue` and `--bin` are aliases). Referenced BIN paths are resolved relative to the CUE file. Opening a raw BIN directly still presents a single-track disc; use the CUE sheet to include CD-DA tracks.

CD-DA playback advances the drive position and emits position reports and end-of-track interrupts. Audio sample mixing is not implemented yet.

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
