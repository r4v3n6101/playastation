use core::iter;

use crate::{
    CPU_FREQ,
    devices::timer::{TimingEvent, TimingSpan},
};

use super::{DEFAULT_HRANGE, DEFAULT_VRANGE, HorizontalResolution, VideoMode};

const GPU_FREQ_NTSC: u64 = 53_693_175;
const GPU_FREQ_PAL: u64 = 53_203_425;
const TICKS_PER_VIDEO_CLOCK: u64 = 4;

const NTSC: VideoStandard = VideoStandard {
    ticks_per_second: GPU_FREQ_NTSC * TICKS_PER_VIDEO_CLOCK,
    ticks_per_scanline: 3412 * TICKS_PER_VIDEO_CLOCK + TICKS_PER_VIDEO_CLOCK / 2,
    dotclocks_per_scanline: [341, 426, 682, 853, 487],
    progressive_half_lines: 263 * 2,
    interlaced_half_lines: 262 * 2 + 1,
};

const PAL: VideoStandard = VideoStandard {
    ticks_per_second: GPU_FREQ_PAL * TICKS_PER_VIDEO_CLOCK,
    ticks_per_scanline: 3405 * TICKS_PER_VIDEO_CLOCK,
    // Timer0 counts integer dots per line; PAL 320 rounds up to 426.
    dotclocks_per_scanline: [340, 426, 681, 851, 486],
    progressive_half_lines: 314 * 2,
    interlaced_half_lines: 312 * 2 + 1,
};

#[derive(Debug, Clone)]
pub struct State {
    line_tick: u64,
    field_tick: u64,
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
    ticks_per_second: u64,
    ticks_per_scanline: u64,
    ticks_per_field: u64,
    dotclocks_per_scanline: u64,
}

struct VideoStandard {
    ticks_per_second: u64,
    ticks_per_scanline: u64,
    dotclocks_per_scanline: [u64; 5],
    progressive_half_lines: u64,
    interlaced_half_lines: u64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            line_tick: 0,
            field_tick: 0,
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
        self.field_tick / self.timing.ticks_per_scanline
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

    pub fn cycles_till_next_event(&self) -> u64 {
        self.cycles_for_ticks(self.ticks_until_next_event())
    }

    pub fn cycles_till_dotclocks(&self, dots: u64) -> u64 {
        if dots == 0 {
            return 0;
        }

        let ticks = (dots * self.timing.ticks_per_scanline - self.dot_remainder)
            .div_ceil(self.timing.dotclocks_per_scanline);

        self.cycles_for_ticks(ticks)
    }

    pub fn update(&mut self, mut remaining: u64) -> impl Iterator<Item = TimingSpan> + '_ {
        iter::from_fn(move || {
            if remaining == 0 {
                return None;
            }

            let step = self.cycles_till_next_event().min(remaining);

            let hblank = self.in_hblank;
            let vblank = self.in_vblank;
            let (dotclocks, event) = self.advance(step);
            remaining -= step;

            Some(TimingSpan {
                sysclocks: step,
                dotclocks,
                hblank,
                vblank,
                event,
            })
        })
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
            || self.timing.ticks_per_scanline != timing.ticks_per_scanline
        {
            self.dot_remainder = 0;
        }
        self.timing = timing;
        self.line_tick %= timing.ticks_per_scanline;
        self.field_tick %= timing.ticks_per_field;

        self.refresh_blank_levels();
    }

    pub fn set_display_ranges(&mut self, hrange: (u16, u16), vrange: (u16, u16)) {
        self.hrange = hrange;
        self.vrange = vrange;

        self.refresh_blank_levels();
    }

    fn refresh_blank_levels(&mut self) {
        self.in_hblank = {
            let (a, b) = self.hrange;
            let unit = TICKS_PER_VIDEO_CLOCK;
            !(u64::from(a) * unit..u64::from(b) * unit).contains(&self.line_tick)
        };
        self.in_vblank = {
            let (a, b) = self.vrange;
            let unit = self.timing.ticks_per_scanline;
            !(u64::from(a) * unit..u64::from(b) * unit).contains(&self.field_tick)
        };
    }

    fn cycles_for_ticks(&self, ticks: u64) -> u64 {
        (ticks * CPU_FREQ - self.clock_remainder).div_ceil(self.timing.ticks_per_second)
    }

    fn advance(&mut self, sysclocks: u64) -> (u64, TimingEvent) {
        let clock = self.clock_remainder + sysclocks * self.timing.ticks_per_second;
        let mut ticks = clock / CPU_FREQ;
        self.clock_remainder = clock % CPU_FREQ;

        self.dot_remainder += ticks * self.timing.dotclocks_per_scanline;
        let dotclocks = self.dot_remainder / self.timing.ticks_per_scanline;
        self.dot_remainder %= self.timing.ticks_per_scanline;

        let mut event = TimingEvent::empty();

        // One CPU clock can cross both edges of a narrow display range.
        while ticks != 0 {
            let step = ticks.min(self.ticks_until_next_event());
            self.line_tick += step;
            self.field_tick += step;
            ticks -= step;

            self.update_blank_edges(&mut event);

            if self.line_tick == self.timing.ticks_per_scanline {
                self.line_tick = 0;
            }
            if self.field_tick == self.timing.ticks_per_field {
                self.field_tick = 0;
                self.even_field = !self.even_field;
            }

            self.update_blank_edges(&mut event);
        }

        (dotclocks, event)
    }

    fn ticks_until_next_event(&self) -> u64 {
        let mut ticks = (self.timing.ticks_per_scanline - self.line_tick)
            .min(self.timing.ticks_per_field - self.field_tick);

        for (position, range, unit) in [
            (self.line_tick, self.hrange, TICKS_PER_VIDEO_CLOCK),
            (self.field_tick, self.vrange, self.timing.ticks_per_scanline),
        ] {
            if range.0 < range.1 {
                for endpoint in [range.0, range.1] {
                    let boundary = u64::from(endpoint) * unit;
                    if boundary > position {
                        ticks = ticks.min(boundary - position);
                    }
                }
            }
        }

        ticks
    }

    fn update_blank_edges(&mut self, event: &mut TimingEvent) {
        update_blank_edge(
            self.line_tick,
            self.hrange,
            TICKS_PER_VIDEO_CLOCK,
            &mut self.in_hblank,
            TimingEvent::HBLANK_ENTER,
            TimingEvent::HBLANK_LEAVE,
            event,
        );

        update_blank_edge(
            self.field_tick,
            self.vrange,
            self.timing.ticks_per_scanline,
            &mut self.in_vblank,
            TimingEvent::VBLANK_ENTER,
            TimingEvent::VBLANK_LEAVE,
            event,
        );
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
            ticks_per_second: standard.ticks_per_second,
            ticks_per_scanline: standard.ticks_per_scanline,
            ticks_per_field: standard.ticks_per_scanline * half_lines / 2,
            dotclocks_per_scanline: standard.dotclocks_per_scanline[index],
        }
    }
}

fn update_blank_edge(
    position: u64,
    (a, b): (u16, u16),
    unit: u64,
    blank: &mut bool,
    enter: TimingEvent,
    leave: TimingEvent,
    event: &mut TimingEvent,
) {
    if a == b {
        return;
    }

    let a = u64::from(a) * unit;
    let b = u64::from(b) * unit;

    if position == a && *blank {
        *blank = false;
        *event |= leave;
    } else if position == b && !*blank {
        *blank = true;
        *event |= enter;
    }
}
