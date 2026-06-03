use core::mem;

use alloc::{boxed::Box, collections::VecDeque};

use modular_bitfield::prelude::*;
use strum::EnumCount;

use crate::{devices::int::InterruptFlags, interconnect::Bus};

use super::{Mmio, read_part, write_part};

pub mod controller;

pub trait SerialDevice {
    fn select(&mut self);
    fn deselect(&mut self);
    fn exchange(&mut self, tx: u8) -> u8;

    fn reset(&mut self) {
        self.deselect();
    }
}

pub struct JoyBus {
    pub mode: JoyMode,
    pub ctrl: JoyCtrl,
    pub baud: u16,

    selected_slot: Slot,
    devs: [Option<Box<dyn SerialDevice>>; Slot::COUNT],
    rx_fifo: VecDeque<u8>,

    irq_pending: bool,
}

#[derive(EnumCount, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    MemCard1,
    Controller1,
    MemCard2,
    Controller2,
}

#[bitfield(bits = 32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    pub ack_irq_enable: bool,

    #[skip]
    __: B1,

    pub reset: bool,

    #[skip]
    __: B1,

    pub rx_irq_mode: B2,
    pub tx_irq_enable: bool,
    pub rx_irq_enable: bool,
    pub ack_irq_enable2: bool,
    pub slot_select: bool,

    #[skip]
    __: B2,
}

impl Default for JoyBus {
    fn default() -> Self {
        Self {
            devs: [const { None }; _],
            selected_slot: Slot::MemCard1,

            rx_fifo: VecDeque::with_capacity(10),

            mode: JoyMode::new(),
            ctrl: JoyCtrl::new(),
            baud: 0,

            irq_pending: false,
        }
    }
}

impl JoyBus {
    pub fn stat(&self) -> JoyStat {
        JoyStat::new()
            .with_tx_ready(true)
            .with_tx_idle(true)
            .with_rx_not_empty(!self.rx_fifo.is_empty())
            .with_irq_pending(self.irq_pending)
    }

    pub fn insert_dev(&mut self, slot: Slot, dev: Box<dyn SerialDevice>) {
        self.devs[slot as usize] = Some(dev);
    }

    pub fn remove_dev(&mut self, slot: Slot) {
        self.devs[slot as usize] = None;
    }

    pub fn update(bus: &mut Bus) {
        if bus.joy_bus.irq_pending {
            bus.int_ctrl.raise(InterruptFlags::JOY);
        }
    }

    fn selected_dev_mut(&mut self) -> &mut Option<Box<dyn SerialDevice>> {
        &mut self.devs[self.selected_slot as usize]
    }

    fn reset(&mut self) {
        self.rx_fifo.clear();
        self.irq_pending = false;

        for port in &mut self.devs {
            if let Some(dev) = port.as_mut() {
                dev.reset();
            }
        }
    }
}

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
                let rx = self.devs[self.selected_slot as usize]
                    .as_mut()
                    .map_or(0xFF, |dev| dev.exchange(tx));

                self.rx_fifo.push_back(rx);

                if self.ctrl.ack_irq_enable() || self.ctrl.ack_irq_enable2() {
                    self.irq_pending = true;
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
                let old = mem::replace(&mut self.ctrl, ctrl);

                if ctrl.reset() {
                    self.reset();
                    return;
                }

                self.selected_slot = match (ctrl.slot_select(), ctrl.joy_select()) {
                    (false, false) => Slot::MemCard1,
                    (false, true) => Slot::Controller1,
                    (true, false) => Slot::MemCard2,
                    (true, true) => Slot::Controller2,
                };

                if !old.joy_select()
                    && ctrl.joy_select()
                    && let Some(dev) = self.selected_dev_mut()
                {
                    dev.select();
                }

                if old.joy_select()
                    && !ctrl.joy_select()
                    && let Some(dev) = self.selected_dev_mut()
                {
                    dev.deselect();
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
