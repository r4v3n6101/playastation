use crate::devices::timer::{TimingEvent, TimingSpan};

use super::{HorizontalResolution, VideoMode};

const NTSC: VideoTiming = VideoTiming {
    sys_cycles_per_scanline: 2152,
    hblank_start: 1722,
    dotclocks_per_scanline: [
        341, // 256
        426, // 320
        682, // 512
        853, // 640
        487, // 368
    ],

    visible_scanlines: 240,
    total_scanlines: 263,
};

const PAL: VideoTiming = VideoTiming {
    sys_cycles_per_scanline: 2168,
    hblank_start: 1734,
    dotclocks_per_scanline: [
        340, // 256
        426, // 320
        681, // 512
        851, // 640
        486, // 368
    ],

    visible_scanlines: 288,
    total_scanlines: 314,
};

#[derive(Debug, Clone)]
pub struct State {
    pub line_cycle: u64,
    pub scanline: u64,
    pub dot_accum: u64,

    special_hres: bool,
    hres: HorizontalResolution,
    timing: VideoTiming,
}

#[derive(Debug, Clone, Copy)]
struct VideoTiming {
    sys_cycles_per_scanline: u64,
    hblank_start: u64,
    dotclocks_per_scanline: [u64; 5],

    visible_scanlines: u64,
    total_scanlines: u64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            line_cycle: 0,
            scanline: 0,
            dot_accum: 0,

            special_hres: false,
            hres: HorizontalResolution::default(),
            timing: NTSC,
        }
    }
}

impl VideoTiming {
    fn dotclocks_per_scanline(&self, hres: HorizontalResolution, special_hres: bool) -> u64 {
        match (special_hres, hres) {
            (true, _) => self.dotclocks_per_scanline[4],
            (_, HorizontalResolution::H256) => self.dotclocks_per_scanline[0],
            (_, HorizontalResolution::H320) => self.dotclocks_per_scanline[1],
            (_, HorizontalResolution::H512) => self.dotclocks_per_scanline[2],
            (_, HorizontalResolution::H640) => self.dotclocks_per_scanline[3],
        }
    }
}

impl State {
    pub fn update(&mut self, sysclocks: u64) -> impl Iterator<Item = TimingSpan> + '_ {
        TimingSpanIter {
            state: self,
            remaining: sysclocks,
        }
    }

    pub fn set_display_mode(
        &mut self,
        mode: VideoMode,
        hres: HorizontalResolution,
        special_hres: bool,
    ) {
        let old_timing = self.timing;
        let old_dots = self
            .timing
            .dotclocks_per_scanline(self.hres, self.special_hres);

        let new_timing = match mode {
            VideoMode::Ntsc => NTSC,
            VideoMode::Pal => PAL,
        };

        let new_dots = new_timing.dotclocks_per_scanline(hres, special_hres);

        self.hres = hres;
        self.special_hres = special_hres;

        if old_timing.sys_cycles_per_scanline != new_timing.sys_cycles_per_scanline {
            self.line_cycle = self.line_cycle * new_timing.sys_cycles_per_scanline
                / old_timing.sys_cycles_per_scanline;
        }

        self.timing = new_timing;

        self.line_cycle %= self.timing.sys_cycles_per_scanline;
        self.scanline %= self.timing.total_scanlines;

        if old_dots != new_dots
            || old_timing.sys_cycles_per_scanline != new_timing.sys_cycles_per_scanline
        {
            self.dot_accum = 0;
        }
    }

    pub fn hblank(&self) -> bool {
        self.line_cycle >= self.timing.hblank_start
    }

    pub fn vblank(&self) -> bool {
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
        self.dot_accum += sysclocks
            * self
                .timing
                .dotclocks_per_scanline(self.hres, self.special_hres);

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
