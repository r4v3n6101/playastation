use core::mem;

use alloc::{boxed::Box, collections::VecDeque};

use modular_bitfield::prelude::*;
use strum::EnumCount;

use crate::{
    SystemTime,
    devices::int::{InterruptController, InterruptFlags},
};

use super::{Mmio, Schedule, read_part, write_part};

pub mod controller;

// Approximate peripheral response delay and ACK pulse width in CPU clocks.
const ACK_DELAY: SystemTime = 100;
const ACK_PULSE: SystemTime = 100;

pub trait SerialDevice {
    /// Starts a new transfer before receiving its address byte.
    fn begin_transfer(&mut self);

    /// # Returns
    ///
    /// Received byte and ACK for the next byte.
    fn exchange(&mut self, tx: u8) -> (u8, bool);
}

pub struct JoyBus {
    pub mode: JoyMode,
    pub ctrl: JoyCtrl,
    pub baud: u16,

    selection: Selection,
    devs: [Option<Box<dyn SerialDevice>>; Slot::COUNT],
    rx_fifo: VecDeque<u8>,

    irq_pending: bool,
    ack_delay: Option<SystemTime>,
    ack_pulse_left: SystemTime,
}

#[derive(Default, Clone, Copy)]
enum Selection {
    #[default]
    Address,
    Device(Slot),
    Disconnected,
}

#[derive(EnumCount, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    MemCard1,
    Controller1,
    MemCard2,
    Controller2,
}

#[bitfield(bits = 32)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct JoyStat {
    pub tx_ready: bool,
    pub rx_not_empty: bool,
    pub tx_idle: bool,
    pub parity_error: bool,

    #[skip]
    __: B3,

    pub ack_input: bool,

    #[skip]
    __: B1,

    pub irq_pending: bool,

    #[skip]
    __: B22,
}

#[bitfield(bits = 16)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct JoyMode {
    pub baud_reload_factor: B2,
    pub char_length: B2,
    pub parity_enable: bool,
    pub parity_type: bool,

    #[skip]
    __: B2,

    pub clock_polarity: bool,

    #[skip]
    __: B7,
}

#[bitfield(bits = 16)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct JoyCtrl {
    pub tx_enable: bool,
    pub joy_select: bool,
    pub rx_enable: bool,

    #[skip]
    __: B1,

    pub acknowledge_irq: bool,

    #[skip]
    __: B1,

    pub reset: bool,

    #[skip]
    __: B1,

    pub rx_irq_mode: B2,
    pub tx_irq_enable: bool,
    pub rx_irq_enable: bool,
    pub ack_irq_enable: bool,
    pub slot_select: bool,

    #[skip]
    __: B2,
}

impl Default for JoyBus {
    fn default() -> Self {
        Self {
            devs: [const { None }; _],
            selection: Selection::default(),

            rx_fifo: VecDeque::with_capacity(16),

            mode: JoyMode::new(),
            ctrl: JoyCtrl::new(),
            baud: 0,

            irq_pending: false,
            ack_delay: None,
            ack_pulse_left: 0,
        }
    }
}

impl JoyBus {
    pub fn stat(&self) -> JoyStat {
        JoyStat::new()
            .with_tx_ready(true)
            .with_tx_idle(true)
            .with_rx_not_empty(!self.rx_fifo.is_empty())
            .with_ack_input(self.ack_pulse_left != 0)
            .with_irq_pending(self.irq_pending)
    }

    // TODO : SmallBox probably
    pub fn insert_dev(&mut self, slot: Slot, dev: Box<dyn SerialDevice>) {
        self.devs[slot as usize] = Some(dev);
    }

    pub fn remove_dev(&mut self, slot: Slot) {
        self.devs[slot as usize] = None;
    }

    pub(crate) fn update(&mut self, int_ctrl: &mut InterruptController, elapsed: SystemTime) {
        self.ack_pulse_left = self.ack_pulse_left.saturating_sub(elapsed);

        let mut ack_edge = false;
        if let Some(delay) = self.ack_delay {
            if delay > elapsed {
                self.ack_delay = Some(delay - elapsed);
            } else {
                self.ack_delay = None;
                self.ack_pulse_left = ACK_PULSE.saturating_sub(elapsed - delay);
                ack_edge = true;
            }
        }

        let irq = (self.ctrl.ack_irq_enable() && (ack_edge || self.ack_pulse_left != 0))
            || self.ctrl.tx_irq_enable()
            || (self.ctrl.rx_irq_enable() && self.rx_fifo.len() >= (1 << self.ctrl.rx_irq_mode()));
        if !self.irq_pending && irq {
            self.irq_pending = true;
            int_ctrl.raise(InterruptFlags::JOY);
        }
    }

    fn exchange(&mut self, tx: u8) -> (u8, bool) {
        const CONTROLLER_ID: u8 = 0x01;
        const MEMCARD_ID: u8 = 0x81;

        if self.ctrl.joy_select() {
            if let Selection::Address = self.selection {
                let slot = match (tx, self.ctrl.slot_select()) {
                    (CONTROLLER_ID, false) => Some(Slot::Controller1),
                    (CONTROLLER_ID, true) => Some(Slot::Controller2),
                    (MEMCARD_ID, false) => Some(Slot::MemCard1),
                    (MEMCARD_ID, true) => Some(Slot::MemCard2),
                    _ => None,
                };
                if let Some(slot) = slot
                    && let Some(dev) = self.devs[slot as usize].as_mut()
                {
                    dev.begin_transfer();
                    self.selection = Selection::Device(slot);
                } else {
                    self.selection = Selection::Disconnected;
                }
            }

            if let Selection::Device(slot) = self.selection
                && let Some(dev) = self.devs[slot as usize].as_mut()
            {
                let response @ (_, ack) = dev.exchange(tx);
                if !ack {
                    self.selection = Selection::Disconnected;
                }

                return response;
            }
        }

        (0xFF, false)
    }

    fn byte_duration(&self) -> SystemTime {
        let factor = match self.mode.baud_reload_factor() {
            0 | 1 => 1,
            2 => 16,
            _ => 64,
        };

        let bit_duration = ((SystemTime::from(self.baud) * factor) & !1).max(1);
        let bits = 5 + u64::from(self.mode.char_length()) + u64::from(self.mode.parity_enable());

        bit_duration * bits
    }
}

impl Schedule for JoyBus {}

impl Mmio for JoyBus {
    fn read(&mut self, dest: &mut [u8], maddr: u32) {
        match maddr {
            0x0..0x4 => {
                read_part::<4, 1>(dest, maddr, [self.rx_fifo.pop_front().unwrap_or(0xFF)]);
            }
            0x4..0x8 => {
                read_part::<4, 4>(dest, maddr, self.stat().into_bytes());
            }
            0x8..0xA => {
                read_part::<2, 2>(dest, maddr, self.mode.into_bytes());
            }
            0xA..0xE => {
                read_part::<2, 2>(dest, maddr, self.ctrl.into_bytes());
            }
            0xE..0x10 => {
                read_part::<2, 2>(dest, maddr, self.baud.to_le_bytes());
            }
            _ => unimplemented!(),
        }
    }

    fn write(&mut self, maddr: u32, value: &[u8]) {
        match maddr {
            0x0..0x4 => {
                if !self.ctrl.tx_enable() {
                    return;
                }

                let [tx] = write_part::<4, 1>(maddr, value, [0]);
                let (rx, ack) = self.exchange(tx);

                if self.ctrl.joy_select() || self.ctrl.rx_enable() {
                    self.rx_fifo.push_back(rx);
                    self.ctrl.set_rx_enable(false);
                }

                if ack {
                    // Delay ACK IRQ
                    self.ack_delay = Some(self.byte_duration() + ACK_DELAY);
                }
            }
            0x4..0x8 => {
                // no-op for stat
            }
            0x8..0xA => {
                self.mode =
                    JoyMode::from_bytes(write_part::<2, 2>(maddr, value, self.mode.into_bytes()));
            }
            0xA..0xE => {
                let ctrl =
                    JoyCtrl::from_bytes(write_part::<2, 2>(maddr, value, self.ctrl.into_bytes()));

                if ctrl.reset() {
                    *self = Self {
                        devs: mem::take(&mut self.devs),
                        ..Self::default()
                    };
                } else {
                    if self.ctrl.joy_select() != ctrl.joy_select()
                        || self.ctrl.slot_select() != ctrl.slot_select()
                    {
                        self.selection = Selection::Address;
                        self.ack_delay = None;
                        self.ack_pulse_left = 0;
                    }

                    if ctrl.acknowledge_irq() {
                        self.irq_pending = false;
                    }
                    self.ctrl = ctrl.with_acknowledge_irq(false).with_reset(false);
                }
            }
            0xE..0x10 => {
                self.baud =
                    u16::from_le_bytes(write_part::<2, 2>(maddr, value, self.baud.to_le_bytes()));
            }
            _ => unimplemented!(),
        }
    }
}
