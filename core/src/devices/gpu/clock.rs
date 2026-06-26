use crate::devices::timer::{TimingEvent, TimingSpan};

pub const NTSC: VideoTiming = VideoTiming {
    sys_cycles_per_scanline: 2152,
    hblank_start: 1722,
    // TODO: this is Timer0 dotclock for 256px display mode only.
    // Real value depends on GP1 horizontal resolution:
    // 256: NTSC 341 / PAL 340
    // 320: NTSC 426 / PAL 426
    // 512: NTSC 682 / PAL 681
    // 640: NTSC 853 / PAL 851
    // 368: NTSC 487 / PAL 486
    dots_per_scanline: 3413,

    visible_scanlines: 240,
    total_scanlines: 263,
};

pub const PAL: VideoTiming = VideoTiming {
    sys_cycles_per_scanline: 2168,
    hblank_start: 1734,
    dots_per_scanline: 3406,

    visible_scanlines: 288,
    total_scanlines: 314,
};

#[derive(Debug, Clone, Copy)]
pub struct VideoTiming {
    pub sys_cycles_per_scanline: u64,
    pub hblank_start: u64,
    pub dots_per_scanline: u64,

    pub visible_scanlines: u64,
    pub total_scanlines: u64,
}

#[derive(Debug, Clone)]
pub struct State {
    pub line_cycle: u64,
    pub scanline: u64,
    pub dot_accum: u64,
    pub timing: VideoTiming,
}

impl State {
    pub fn new(timing: VideoTiming) -> Self {
        Self {
            line_cycle: 0,
            scanline: 0,
            dot_accum: 0,
            timing,
        }
    }

    pub fn update(&mut self, sysclocks: u64) -> impl Iterator<Item = TimingSpan> + '_ {
        TimingSpanIter {
            state: self,
            remaining: sysclocks,
        }
    }

    fn hblank(&self) -> bool {
        self.line_cycle >= self.timing.hblank_start
    }

    fn vblank(&self) -> bool {
        self.scanline >= self.timing.visible_scanlines
    }

    fn cycles_until_next_event(&self) -> u64 {
        let line_end = self.timing.sys_cycles_per_scanline - self.line_cycle;

        let hblank_event = if self.hblank() {
            // Leave HBlank at next scanline.
            line_end
        } else {
            // Enter HBlank in current scanline.
            self.timing.hblank_start - self.line_cycle
        };

        let vblank_event = if self.vblank() {
            // Leave VBlank at frame wrap.
            let lines_left = self.timing.total_scanlines - self.scanline - 1;
            lines_left * self.timing.sys_cycles_per_scanline + line_end
        } else {
            // Enter VBlank at first invisible scanline.
            let lines_left = self.timing.visible_scanlines - self.scanline - 1;
            lines_left * self.timing.sys_cycles_per_scanline + line_end
        };

        hblank_event.min(vblank_event).max(1)
    }

    fn advance_scan(&mut self, sysclocks: u64) {
        let total = self.line_cycle + sysclocks;
        let lines = total / self.timing.sys_cycles_per_scanline;

        self.line_cycle = total % self.timing.sys_cycles_per_scanline;
        self.scanline = (self.scanline + lines) % self.timing.total_scanlines;
    }

    fn advance_dotclock(&mut self, sysclocks: u64) -> u64 {
        self.dot_accum += sysclocks * self.timing.dots_per_scanline;

        let dotclocks = self.dot_accum / self.timing.sys_cycles_per_scanline;
        self.dot_accum %= self.timing.sys_cycles_per_scanline;

        dotclocks
    }
}

struct TimingSpanIter<'a> {
    state: &'a mut State,
    remaining: u64,
}

impl Iterator for TimingSpanIter<'_> {
    type Item = TimingSpan;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let step = self.state.cycles_until_next_event().min(self.remaining);

        let old_hblank = self.state.hblank();
        let old_vblank = self.state.vblank();

        let dotclocks = self.state.advance_dotclock(step);
        self.state.advance_scan(step);

        self.remaining -= step;

        let new_hblank = self.state.hblank();
        let new_vblank = self.state.vblank();

        let mut event = TimingEvent::empty();
        if !old_hblank && new_hblank {
            event |= TimingEvent::HBLANK_ENTER;
        }
        if old_hblank && !new_hblank {
            event |= TimingEvent::HBLANK_LEAVE;
        }
        if !old_vblank && new_vblank {
            event |= TimingEvent::VBLANK_ENTER;
        }
        if old_vblank && !new_vblank {
            event |= TimingEvent::VBLANK_LEAVE;
        }

        Some(TimingSpan {
            sysclocks: step,
            dotclocks,
            hblank: old_hblank,
            vblank: old_vblank,
            event,
        })
    }
}
