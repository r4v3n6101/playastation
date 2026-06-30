use std::{
    fs, hint,
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
use triple_buffer::{Input, Output};

use crate::app::{EmulatorData, EmulatorHost, InputState};

#[derive(Parser)]
pub struct Args {
    #[arg(long)]
    bios: PathBuf,
    #[arg(long)]
    rom: Option<PathBuf>,
    #[arg(long)]
    bin: Option<PathBuf>,
}

pub struct TimeScaler<const GUEST_FREQ: u64> {
    started_at: Instant,
    emulated_cycles: u64,
}

pub struct Host {
    button_state: Arc<AtomicCell<Button>>,
    data_rx: Output<EmulatorData>,
}

impl<const GUEST_FREQ: u64> Default for TimeScaler<GUEST_FREQ> {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
            emulated_cycles: 0,
        }
    }
}

impl Host {
    pub fn new(button_state: Arc<AtomicCell<Button>>, data_rx: Output<EmulatorData>) -> Self {
        Self {
            button_state,
            data_rx,
        }
    }
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

impl<const GUEST_FREQ: u64> TimeScaler<GUEST_FREQ> {
    pub fn emu_elapsed(&self) -> Duration {
        Duration::from_secs_f64(self.emulated_cycles as f64 / GUEST_FREQ as f64)
    }

    pub fn host_elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    pub fn ahead_by(&self) -> Option<Duration> {
        self.emu_elapsed().checked_sub(self.host_elapsed())
    }

    pub fn add_cycles(&mut self, cycles: u64) {
        self.emulated_cycles = self.emulated_cycles.saturating_add(cycles);
    }

    pub fn wait(&mut self) {
        const SLEEP_THRESHOLD: Duration = Duration::from_millis(3);
        const SLEEP_MARGIN: Duration = Duration::from_millis(1);
        const YIELD_THRESHOLD: Duration = Duration::from_micros(300);

        if let Some(ahead) = self.ahead_by()
            && ahead < SLEEP_THRESHOLD
        {
            return;
        }

        while let Some(ahead) = self.ahead_by() {
            if ahead > SLEEP_THRESHOLD {
                thread::sleep(ahead - SLEEP_MARGIN);
            } else if ahead > YIELD_THRESHOLD {
                thread::yield_now();
            } else {
                hint::spin_loop();
            }
        }

        self.started_at = Instant::now();
        self.emulated_cycles = 0;
    }
}

pub fn spawn_emulator_thread(
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

        let mut scaler = TimeScaler::<CPU_FREQ>::default();
        let mut last_frame = Instant::now();
        loop {
            let sys_cycles = executor.run();

            scaler.add_cycles(sys_cycles);
            scaler.wait();

            if last_frame.elapsed() > Duration::from_secs(1) / 90 {
                data_tx.write(EmulatorData {
                    display_width: VRAM_WIDTH,
                    display_height: VRAM_HEIGHT,
                    display_pixels: executor.bus.gpu.renderer.framebuffer().to_vec(),

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
