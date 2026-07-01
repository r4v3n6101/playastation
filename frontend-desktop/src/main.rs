use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use clap::Parser;
use crossbeam_utils::atomic::AtomicCell;
use playastation::{
    CPU_FREQ, VRAM_HEIGHT, VRAM_WIDTH,
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
use playastation_frontend_common::{App, EmulatorData, EmulatorHost, InputState, eframe, egui};
use triple_buffer::{Input, Output};

mod time;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    bios: PathBuf,
    #[arg(long)]
    rom: Option<PathBuf>,
    #[arg(long)]
    bin: Option<PathBuf>,
}

struct Host {
    button_state: Arc<AtomicCell<Button>>,
    data_rx: Output<EmulatorData>,
}

impl EmulatorHost for Host {
    fn send_input(&mut self, input: InputState) {
        self.button_state.store(input.buttons);
    }

    fn poll_data(&mut self) -> Option<EmulatorData> {
        if !self.data_rx.updated() {
            return None;
        }

        Some(self.data_rx.read().clone())
    }
}

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(true)
        .with_level(true)
        .compact()
        .init();

    let args = Args::parse();

    let (data_tx, data_rx) = triple_buffer::triple_buffer(&EmulatorData::default());
    let button_state = Arc::new(AtomicCell::new(Button::empty()));

    spawn_emulator_thread(args, Arc::clone(&button_state), data_tx);

    eframe::run_native(
        "PlayaStation",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("PlayaStation")
                .with_inner_size([1200.0, 800.0]),
            ..Default::default()
        },
        Box::new(move |_cc| {
            Ok(Box::new(App::new(Host {
                button_state,
                data_rx,
            })))
        }),
    )
}

fn spawn_emulator_thread(
    args: Args,
    button_state: Arc<AtomicCell<Button>>,
    mut data_tx: Input<EmulatorData>,
) {
    thread::spawn(move || {
        let mut executor = Executor::default();

        executor
            .bus
            .bios
            .copy_from_slice(&fs::read(&args.bios).unwrap());

        if let Some(rom_path) = &args.rom {
            executor.pending_exe = Some(BoxedExeFile::new(
                fs::read(rom_path).unwrap().into_boxed_slice(),
            ));
        }

        if let Some(bin_path) = &args.bin {
            executor.bus.cdrom.disc = Some(Box::new(BinFile {
                data: fs::read(bin_path).unwrap(),
            }));
        }

        executor.bus.gpu.renderer = Box::new(SoftwareRenderer::default());

        executor.bus.joy_bus.insert_dev(
            Slot::Controller1,
            Box::new(DigitalController::with_poll_buttons(Box::new(move || {
                button_state.load()
            }))),
        );

        let mut scaler = time::Scaler::<CPU_FREQ>::default();
        let mut last_frame = Instant::now();
        loop {
            let sys_cycles = executor.run();

            scaler.add_cycles(sys_cycles);
            scaler.wait();

            if last_frame.elapsed() > Duration::from_millis(5) {
                data_tx.write(EmulatorData {
                    vram: executor.bus.gpu.renderer.framebuffer().to_vec(),
                    vram_start: executor.bus.gpu.vram_start,
                    display: executor.bus.gpu.display,

                    cdrom_status: executor.bus.cdrom.status,
                    cdrom_mode: executor.bus.cdrom.mode,
                    cdrom_stat: executor.bus.cdrom.stat(),

                    gpu_stat: executor.bus.gpu.stat(),

                    joy_baud: executor.bus.joy_bus.baud,
                    joy_mode: executor.bus.joy_bus.mode,
                    joy_ctrl: executor.bus.joy_bus.ctrl,
                    joy_stat: executor.bus.joy_bus.stat(),
                });
                last_frame = Instant::now();
            }
        }
    });
}

// TODO : move to core
struct BinFile {
    data: Vec<u8>,
}

impl Disc for BinFile {
    fn read_sector(&mut self, lba: usize) -> Option<RawSector> {
        let chunk = self.data.get(lba * 2352 + 12..)?.get(..2340)?;

        Some(chunk.try_into().unwrap())
    }

    fn sector_count(&self) -> usize {
        self.data.len() / 2352
    }
}
