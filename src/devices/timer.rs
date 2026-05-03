use modular_bitfield::prelude::*;

use super::{Mmio, MmioExt};

const TIMERS: usize = 3;

#[derive(Debug, Default)]
pub struct TimerController {
    pub timers: [Timer; TIMERS],
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
#[derive(Specifier, Debug, Default, Clone, Copy, PartialEq, Eq)]
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
    pub irq_request: bool,
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

impl Mmio for TimerController {
    fn read(&mut self, dest: &mut [u8], addr: u32) {
        self.read_unaligned(dest, addr, |this, addr| {
            let timer = (addr / 0x10) as usize;
            let reg = addr % 0x10;

            match reg {
                0x0 => u32::from(this.timers[timer].counter),
                0x4 => {
                    let timer = &mut this.timers[timer];
                    let val = u16::from_le_bytes(timer.mode.into_bytes());

                    timer.mode.set_reached_target(false);
                    timer.mode.set_reached_overflow(false);

                    u32::from(val)
                }
                0x8 => u32::from(this.timers[timer].target),
                _ => unreachable!(),
            }
        });
    }

    fn write(&mut self, addr: u32, value: &[u8]) {
        let (addr, val) = self.write_value(addr, value);
        let timer = (addr / 0x10) as usize;
        let reg = addr % 0x10;

        match reg {
            0x0 => self.timers[timer].counter = val as u16,
            0x4 => {
                let timer = &mut self.timers[timer];
                timer.counter = 0;
                timer.mode = TimerMode::from_bytes((val as u16).to_le_bytes())
                    .with_irq_request(true)
                    .with_reached_target(false)
                    .with_reached_overflow(false);
            }
            0x8 => self.timers[timer].target = val as u16,
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{super::Mmio, TimerController};

    fn read(ctrl: &mut TimerController, addr: u32) -> u32 {
        let mut buf = [0; 4];
        ctrl.read(&mut buf, addr);
        u32::from_le_bytes(buf)
    }

    fn write(ctrl: &mut TimerController, addr: u32, val: u32) {
        ctrl.write(addr, val.to_le_bytes().as_slice());
    }

    #[test]
    fn write_mode_resets_counter_and_sets_irq_request() {
        let mut ctrl = TimerController::default();

        write(&mut ctrl, 0x0, 0x1234);
        write(&mut ctrl, 0x4, 0x0038);

        assert_eq!(ctrl.timers[0].counter, 0);
        assert!(ctrl.timers[0].mode.irq_request());
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
