use core::cmp::Ordering;

use modular_bitfield::prelude::*;

use crate::{devices::int::InterruptFlags, interconnect::Bus};

use super::{Mmio, read_part, write_part};

const TIMERS: usize = 3;
const COUNTER_PERIOD: u64 = u16::MAX as u64 + 1;

#[derive(Debug, Default, Clone, Copy)]
pub struct TimerInput {
    pub sysclocks: u64,
    pub dotclocks: u64,
    pub hblanks: u64,
}

#[derive(Debug, Default)]
pub struct TimerController {
    pub timers: [Timer; TIMERS],
    sysclock_8_rem: u64,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Timer {
    /// Current counter value.
    pub counter: u16,
    /// Counter mode.
    pub mode: TimerMode,
    /// Counter target value.
    pub target: u16,
}

#[bitfield(bits = 16)]
#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimerMode {
    /// Synchronize counter with HBlank/VBlank depending on timer index.
    pub sync_enabled: bool,
    /// Synchronization mode. Meaning depends on timer index.
    pub sync_mode: SyncMode,
    /// Reset at target instead of overflowing after 0xFFFF.
    pub reset_on_target: bool,
    /// IRQ when the counter reaches target.
    pub irq_on_target: bool,
    /// IRQ when the counter reaches 0xFFFF.
    pub irq_on_overflow: bool,
    /// IRQ repeat mode.
    pub irq_repeat: bool,
    /// IRQ toggle mode.
    pub irq_toggle: bool,
    /// Clock source. Meaning depends on timer index.
    pub clock_source: ClockSource,
    /// Interrupt request line status: 0 = request, 1 = no request.
    pub irq_inhibit: bool,
    /// Latched when the counter reaches target; cleared after mode read.
    pub reached_target: bool,
    /// Latched when the counter reaches 0xFFFF; cleared after mode read.
    pub reached_overflow: bool,
    #[skip]
    reserved: B3,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 2]
pub enum SyncMode {
    Mode0 = 0,
    Mode1 = 1,
    Mode2 = 2,
    Mode3 = 3,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 2]
pub enum ClockSource {
    Source0 = 0,
    Source1 = 1,
    Source2 = 2,
    Source3 = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimerEvent {
    Target,
    Overflow,
    TargetAndOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TimerStep {
    ticks: u64,
    event: Option<TimerEvent>,
}

impl Default for TimerMode {
    fn default() -> Self {
        Self::new().with_irq_inhibit(true)
    }
}

impl Timer {
    fn advance(&mut self, mut ticks: u64) -> bool {
        let mut irq = false;

        while ticks > 0 {
            let step = self.next_step(ticks);
            debug_assert!((0..=COUNTER_PERIOD).contains(&step.ticks));

            irq |= self.apply_step(step);

            ticks -= step.ticks;
        }

        irq
    }

    fn next_step(&self, remaining: u64) -> TimerStep {
        let target_ticks = self.ticks_until_target();
        let overflow_ticks = self.ticks_until_overflow();

        let (event_ticks, event) = if self.mode.reset_on_target() {
            // Target resets counter, so overflow cannot happen after target
            // in the same counter period.
            (target_ticks, TimerEvent::Target)
        } else {
            match target_ticks.cmp(&overflow_ticks) {
                Ordering::Less => (target_ticks, TimerEvent::Target),
                Ordering::Greater => (overflow_ticks, TimerEvent::Overflow),
                Ordering::Equal => (target_ticks, TimerEvent::TargetAndOverflow),
            }
        };

        if remaining < event_ticks {
            TimerStep {
                ticks: remaining,
                event: None,
            }
        } else {
            TimerStep {
                ticks: event_ticks,
                event: Some(event),
            }
        }
    }

    fn apply_step(&mut self, step: TimerStep) -> bool {
        self.counter = self.counter.wrapping_add(step.ticks as u16);

        match step.event {
            None => false,
            Some(TimerEvent::Target) => self.on_target(),
            Some(TimerEvent::Overflow) => self.on_overflow(),
            Some(TimerEvent::TargetAndOverflow) => {
                let mut irq = false;
                irq |= self.on_target();
                irq |= self.on_overflow();
                irq
            }
        }
    }

    fn ticks_until_target(&self) -> u64 {
        let dist = self.target.wrapping_sub(self.counter);

        if dist == 0 {
            COUNTER_PERIOD
        } else {
            u64::from(dist)
        }
    }

    fn ticks_until_overflow(&self) -> u64 {
        COUNTER_PERIOD - u64::from(self.counter)
    }

    fn on_target(&mut self) -> bool {
        self.mode.set_reached_target(true);

        if self.mode.reset_on_target() {
            self.counter = 0;
        }

        self.mode.irq_on_target() && self.trigger_irq()
    }

    fn on_overflow(&mut self) -> bool {
        self.mode.set_reached_overflow(true);

        self.mode.irq_on_overflow() && self.trigger_irq()
    }

    fn trigger_irq(&mut self) -> bool {
        // One-shot already fired: bit10 is already 0/requested.
        // Further IRQs suppressed until mode write resets bit10 to 1.
        if !self.mode.irq_repeat() && !self.mode.irq_inhibit() {
            return false;
        }

        if self.mode.irq_toggle() {
            self.mode.set_irq_inhibit(!self.mode.irq_inhibit());

            // IRQ line active only on transition/result to 0.
            !self.mode.irq_inhibit()
        } else {
            // Pulse mode: hardware pulses bit10=0 briefly.
            self.mode.set_irq_inhibit(false);

            if self.mode.irq_repeat() {
                // Approximation: pulse ends immediately.
                self.mode.set_irq_inhibit(true);
            }

            // Raise external IRQ immediately.
            true
        }
    }
}

impl TimerController {
    pub fn update(bus: &mut Bus, input: TimerInput) {
        let timer2_div8 = {
            let ctrl = &mut bus.timer_ctrl;
            ctrl.sysclock_8_rem += input.sysclocks;

            let ticks = ctrl.sysclock_8_rem / 8;
            ctrl.sysclock_8_rem %= 8;

            ticks
        };

        for i in 0..TIMERS {
            let timer = &mut bus.timer_ctrl.timers[i];

            let count = match (i, timer.mode.clock_source()) {
                (0, ClockSource::Source0 | ClockSource::Source1) => input.sysclocks,
                (0, ClockSource::Source2 | ClockSource::Source3) => input.dotclocks,

                (1, ClockSource::Source0 | ClockSource::Source1) => input.sysclocks,
                (1, ClockSource::Source2 | ClockSource::Source3) => input.hblanks,

                (2, ClockSource::Source0 | ClockSource::Source1) => input.sysclocks,
                (2, ClockSource::Source2 | ClockSource::Source3) => timer2_div8,

                _ => unreachable!(),
            };

            let irq = timer.advance(count);
            if irq {
                bus.int_ctrl.raise(match i {
                    0 => InterruptFlags::TMR0,
                    1 => InterruptFlags::TMR1,
                    2 => InterruptFlags::TMR2,
                    _ => unreachable!(),
                });
            }
        }
    }
}

impl Mmio for TimerController {
    fn read(&mut self, dest: &mut [u8], maddr: u32) {
        let timer = (maddr / 0x10) as usize;
        let reg = maddr % 0x10;

        match reg {
            0x0..0x4 => {
                read_part::<4, 2>(dest, maddr, self.timers[timer].counter.to_le_bytes());
            }
            0x4..0x8 => {
                let timer = &mut self.timers[timer];
                let val = timer.mode.into_bytes();

                timer.mode.set_reached_target(false);
                timer.mode.set_reached_overflow(false);

                read_part::<4, 2>(dest, maddr, val);
            }
            0x8..0xC => {
                read_part::<4, 2>(dest, maddr, self.timers[timer].target.to_le_bytes());
            }
            _ => unimplemented!(),
        }
    }

    fn write(&mut self, maddr: u32, value: &[u8]) {
        let timer = (maddr / 0x10) as usize;
        let reg = maddr % 0x10;

        match reg {
            0x0 => {
                self.timers[timer].counter = u16::from_le_bytes(write_part::<4, 2>(
                    maddr,
                    value,
                    self.timers[timer].counter.to_le_bytes(),
                ));
            }
            0x4 => {
                let timer = &mut self.timers[timer];
                timer.counter = 0;

                timer.mode = TimerMode::from_bytes(write_part::<4, 2>(
                    maddr,
                    value,
                    timer.mode.into_bytes(),
                ))
                .with_irq_inhibit(true)
                .with_reached_target(false)
                .with_reached_overflow(false);
            }
            0x8 => {
                self.timers[timer].target = u16::from_le_bytes(write_part::<4, 2>(
                    maddr,
                    value,
                    self.timers[timer].target.to_le_bytes(),
                ));
            }
            _ => unimplemented!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{super::Mmio, TimerController, TimerMode};

    fn read(ctrl: &mut TimerController, maddr: u32) -> u32 {
        let mut buf = [0; 4];
        ctrl.read(&mut buf, maddr);
        u32::from_le_bytes(buf)
    }

    fn write(ctrl: &mut TimerController, maddr: u32, val: u32) {
        ctrl.write(maddr, val.to_le_bytes().as_slice());
    }

    #[test]
    fn verify_default_mode() {
        let reg = u16::from_le_bytes(TimerMode::default().into_bytes());

        assert_eq!(reg, 0x0400);
    }

    #[test]
    fn write_mode_resets_counter_and_sets_irq_request() {
        let mut ctrl = TimerController::default();

        write(&mut ctrl, 0x0, 0x1234);
        write(&mut ctrl, 0x4, 0x0038);

        assert_eq!(ctrl.timers[0].counter, 0);
        assert!(ctrl.timers[0].mode.irq_inhibit());
        assert!(ctrl.timers[0].mode.reset_on_target());
        assert!(ctrl.timers[0].mode.irq_on_target());
        assert!(ctrl.timers[0].mode.irq_on_overflow());
    }

    #[test]
    fn mode_read_clears_reached_flags() {
        let mut ctrl = TimerController::default();
        ctrl.timers[1].mode.set_reached_target(true);
        ctrl.timers[1].mode.set_reached_overflow(true);

        assert_eq!(read(&mut ctrl, 0x14) & 0x1800, 0x1800);
        assert!(!ctrl.timers[1].mode.reached_target());
        assert!(!ctrl.timers[1].mode.reached_overflow());
    }

    #[test]
    fn registers_are_repeated_for_three_timers() {
        let mut ctrl = TimerController::default();

        write(&mut ctrl, 0x20, 0x1111);
        write(&mut ctrl, 0x28, 0x2222);

        assert_eq!(read(&mut ctrl, 0x20), 0x1111);
        assert_eq!(read(&mut ctrl, 0x28), 0x2222);
    }
}
