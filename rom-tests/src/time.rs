use std::{
    hint, thread,
    time::{Duration, Instant},
};

pub struct Scaler<const GUEST_FREQ: u64> {
    started_at: Instant,
    emulated_cycles: u64,
}

impl<const GUEST_FREQ: u64> Default for Scaler<GUEST_FREQ> {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
            emulated_cycles: 0,
        }
    }
}

impl<const GUEST_FREQ: u64> Scaler<GUEST_FREQ> {
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
