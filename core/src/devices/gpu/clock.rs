use core::iter;

use bitflags::bitflags;

use crate::{
    CPU_FREQ, SystemTime,
    devices::timer::{TimingEvent, TimingSpan},
};

use super::{DEFAULT_HRANGE, DEFAULT_VRANGE, HorizontalResolution, VideoMode};

const GPU_FREQ_NTSC: u64 = 53_693_175;
const GPU_FREQ_PAL: u64 = 53_203_425;
// Internal positions count quarter video clocks.
const QUARTERS_PER_VIDEO_CLOCK: u64 = 4;

const NTSC: VideoStandard = VideoStandard {
    quarter_clocks_per_second: GPU_FREQ_NTSC * QUARTERS_PER_VIDEO_CLOCK,
    quarter_clocks_per_scanline: 3412 * QUARTERS_PER_VIDEO_CLOCK + QUARTERS_PER_VIDEO_CLOCK / 2,
    dotclocks_per_scanline: [341, 426, 682, 853, 487],
    progressive_half_lines: 263 * 2,
    interlaced_half_lines: 262 * 2 + 1,
};

const PAL: VideoStandard = VideoStandard {
    quarter_clocks_per_second: GPU_FREQ_PAL * QUARTERS_PER_VIDEO_CLOCK,
    quarter_clocks_per_scanline: 3405 * QUARTERS_PER_VIDEO_CLOCK,
    // Timer0 counts integer dots per line; PAL 320 rounds up to 426.
    dotclocks_per_scanline: [340, 426, 681, 851, 486],
    progressive_half_lines: 314 * 2,
    interlaced_half_lines: 312 * 2 + 1,
};

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Boundaries: u8 {
        const HBLANK_ENTER = 1 << 0;
        const HBLANK_LEAVE = 1 << 1;
        const VBLANK_ENTER = 1 << 2;
        const VBLANK_LEAVE = 1 << 3;
        const LINE_WRAP = 1 << 4;
        const FIELD_WRAP = 1 << 5;
    }
}

#[derive(Debug, Clone)]
pub struct State {
    line_quarter_clocks: u64,
    field_quarter_clocks: u64,
    clock_remainder: u64,
    dot_remainder: u64,
    in_hblank: bool,
    in_vblank: bool,
    even_field: bool,

    hrange: (u16, u16),
    vrange: (u16, u16),
    timing: VideoTiming,
}

#[derive(Debug, Clone, Copy)]
struct VideoTiming {
    quarter_clocks_per_second: u64,
    quarter_clocks_per_scanline: u64,
    quarter_clocks_per_field: u64,
    dotclocks_per_scanline: u64,
}

struct VideoStandard {
    quarter_clocks_per_second: u64,
    quarter_clocks_per_scanline: u64,
    dotclocks_per_scanline: [u64; 5],
    progressive_half_lines: u64,
    interlaced_half_lines: u64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            line_quarter_clocks: 0,
            field_quarter_clocks: 0,
            clock_remainder: 0,
            dot_remainder: 0,
            in_hblank: true,
            in_vblank: true,
            even_field: true,

            hrange: DEFAULT_HRANGE,
            vrange: DEFAULT_VRANGE,

            timing: VideoTiming::new(VideoMode::Ntsc, HorizontalResolution::H256, false, false),
        }
    }
}

impl State {
    pub fn scanline(&self) -> u64 {
        self.field_quarter_clocks / self.timing.quarter_clocks_per_scanline
    }

    pub fn vblank(&self) -> bool {
        self.in_vblank
    }

    pub fn hblank(&self) -> bool {
        self.in_hblank
    }

    pub fn even_field(&self) -> bool {
        self.even_field
    }

    pub fn delay_till_next_event(&self) -> SystemTime {
        self.duration_for_quarter_clocks(self.next_boundary().0)
    }

    pub fn delay_till_dotclocks(&self, dots: u64) -> SystemTime {
        if dots == 0 {
            return 0;
        }

        let quarter_clocks = (dots * self.timing.quarter_clocks_per_scanline - self.dot_remainder)
            .div_ceil(self.timing.dotclocks_per_scanline);

        self.duration_for_quarter_clocks(quarter_clocks)
    }

    pub fn set_display_mode(
        &mut self,
        mode: VideoMode,
        hres: HorizontalResolution,
        special_hres: bool,
        interlaced: bool,
    ) {
        let timing = VideoTiming::new(mode, hres, special_hres, interlaced);

        // idk but i think it must be reset when changing format
        if self.timing.dotclocks_per_scanline != timing.dotclocks_per_scanline
            || self.timing.quarter_clocks_per_scanline != timing.quarter_clocks_per_scanline
        {
            self.dot_remainder = 0;
        }
        self.timing = timing;
        self.line_quarter_clocks %= timing.quarter_clocks_per_scanline;
        self.field_quarter_clocks %= timing.quarter_clocks_per_field;

        self.refresh_blank_levels();
    }

    pub fn set_display_ranges(&mut self, hrange: (u16, u16), vrange: (u16, u16)) {
        self.hrange = hrange;
        self.vrange = vrange;

        self.refresh_blank_levels();
    }

    pub fn update(&mut self, elapsed: SystemTime) -> impl Iterator<Item = TimingSpan> + '_ {
        let mut remaining = elapsed;
        iter::from_fn(move || {
            let (mut quarter_clocks, mut boundaries) = self.next_boundary();
            let delay = self.duration_for_quarter_clocks(quarter_clocks);

            if remaining == 0 && delay != 0 && self.clock_remainder < CPU_FREQ {
                return None;
            }

            let duration = remaining.min(delay);
            if duration < delay {
                quarter_clocks = (self.clock_remainder
                    + duration * self.timing.quarter_clocks_per_second)
                    / CPU_FREQ;
                boundaries = Boundaries::empty();
            }

            remaining -= duration;
            Some(self.advance(duration, quarter_clocks, boundaries))
        })
    }

    fn advance(
        &mut self,
        elapsed: SystemTime,
        quarter_clocks: u64,
        boundaries: Boundaries,
    ) -> TimingSpan {
        let mut span = TimingSpan {
            elapsed,
            dotclocks: 0,
            hblank: self.in_hblank,
            vblank: self.in_vblank,
            event: TimingEvent::empty(),
        };

        self.clock_remainder += elapsed * self.timing.quarter_clocks_per_second;
        self.clock_remainder -= quarter_clocks * CPU_FREQ;

        self.dot_remainder += quarter_clocks * self.timing.dotclocks_per_scanline;
        span.dotclocks = self.dot_remainder / self.timing.quarter_clocks_per_scanline;
        self.dot_remainder %= self.timing.quarter_clocks_per_scanline;

        self.line_quarter_clocks += quarter_clocks;
        self.field_quarter_clocks += quarter_clocks;

        if boundaries.contains(Boundaries::HBLANK_ENTER) && !self.in_hblank {
            self.in_hblank = true;
            span.event |= TimingEvent::HBLANK_ENTER;
        }
        if boundaries.contains(Boundaries::VBLANK_ENTER) && !self.in_vblank {
            self.in_vblank = true;
            span.event |= TimingEvent::VBLANK_ENTER;
        }

        if boundaries.contains(Boundaries::LINE_WRAP) {
            self.line_quarter_clocks = 0;
        }
        if boundaries.contains(Boundaries::FIELD_WRAP) {
            self.field_quarter_clocks = 0;
            self.even_field = !self.even_field;
        }

        if boundaries.contains(Boundaries::HBLANK_LEAVE) && self.in_hblank {
            self.in_hblank = false;
            span.event |= TimingEvent::HBLANK_LEAVE;
        }
        if boundaries.contains(Boundaries::VBLANK_LEAVE) && self.in_vblank {
            self.in_vblank = false;
            span.event |= TimingEvent::VBLANK_LEAVE;
        }

        span
    }

    fn next_boundary(&self) -> (u64, Boundaries) {
        let mut quarter_clocks = self.timing.quarter_clocks_per_scanline - self.line_quarter_clocks;
        let mut boundaries = Boundaries::LINE_WRAP;

        let mut include = |distance, boundary| {
            if distance < quarter_clocks {
                quarter_clocks = distance;
                boundaries = boundary;
            } else if distance == quarter_clocks {
                boundaries |= boundary;
            }
        };

        include(
            self.timing.quarter_clocks_per_field - self.field_quarter_clocks,
            Boundaries::FIELD_WRAP,
        );

        for (position, range, unit, leave, enter) in [
            (
                self.line_quarter_clocks,
                self.hrange,
                QUARTERS_PER_VIDEO_CLOCK,
                Boundaries::HBLANK_LEAVE,
                Boundaries::HBLANK_ENTER,
            ),
            (
                self.field_quarter_clocks,
                self.vrange,
                self.timing.quarter_clocks_per_scanline,
                Boundaries::VBLANK_LEAVE,
                Boundaries::VBLANK_ENTER,
            ),
        ] {
            if range.0 < range.1 {
                for (endpoint, boundary) in [(range.0, leave), (range.1, enter)] {
                    let endpoint = u64::from(endpoint) * unit;
                    if endpoint > position {
                        include(endpoint - position, boundary);
                    }
                }
            }
        }

        // A range starting at zero leaves blank immediately after the wrap.
        if boundaries.contains(Boundaries::LINE_WRAP) && self.hrange.0 == 0 && self.hrange.1 != 0 {
            boundaries |= Boundaries::HBLANK_LEAVE;
        }
        if boundaries.contains(Boundaries::FIELD_WRAP) && self.vrange.0 == 0 && self.vrange.1 != 0 {
            boundaries |= Boundaries::VBLANK_LEAVE;
        }

        (quarter_clocks, boundaries)
    }

    fn refresh_blank_levels(&mut self) {
        self.in_hblank = {
            let (a, b) = self.hrange;
            let unit = QUARTERS_PER_VIDEO_CLOCK;
            !(u64::from(a) * unit..u64::from(b) * unit).contains(&self.line_quarter_clocks)
        };
        self.in_vblank = {
            let (a, b) = self.vrange;
            let unit = self.timing.quarter_clocks_per_scanline;
            !(u64::from(a) * unit..u64::from(b) * unit).contains(&self.field_quarter_clocks)
        };
    }

    fn duration_for_quarter_clocks(&self, quarter_clocks: u64) -> SystemTime {
        (quarter_clocks * CPU_FREQ)
            .saturating_sub(self.clock_remainder)
            .div_ceil(self.timing.quarter_clocks_per_second)
    }
}

impl VideoTiming {
    fn new(
        mode: VideoMode,
        hres: HorizontalResolution,
        special_hres: bool,
        interlaced: bool,
    ) -> Self {
        let standard = match mode {
            VideoMode::Ntsc => &NTSC,
            VideoMode::Pal => &PAL,
        };

        let half_lines = if interlaced {
            standard.interlaced_half_lines
        } else {
            standard.progressive_half_lines
        };

        let index = if special_hres {
            4
        } else {
            match hres {
                HorizontalResolution::H256 => 0,
                HorizontalResolution::H320 => 1,
                HorizontalResolution::H512 => 2,
                HorizontalResolution::H640 => 3,
            }
        };

        Self {
            quarter_clocks_per_second: standard.quarter_clocks_per_second,
            quarter_clocks_per_scanline: standard.quarter_clocks_per_scanline,
            quarter_clocks_per_field: standard.quarter_clocks_per_scanline * half_lines / 2,
            dotclocks_per_scanline: standard.dotclocks_per_scanline[index],
        }
    }
}
