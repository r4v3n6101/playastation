use alloc::boxed::Box;
use core::mem;

use super::SerialDevice;

type ButtonPressedCallback = Box<dyn FnMut() -> Button>;

bitflags::bitflags! {
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct Button: u16 {
        const SELECT   = 1 << 0;
        const L3       = 1 << 1;
        const R3       = 1 << 2;
        const START    = 1 << 3;
        const UP       = 1 << 4;
        const RIGHT    = 1 << 5;
        const DOWN     = 1 << 6;
        const LEFT     = 1 << 7;
        const L2       = 1 << 8;
        const R2       = 1 << 9;
        const L1       = 1 << 10;
        const R1       = 1 << 11;
        const TRIANGLE = 1 << 12;
        const CIRCLE   = 1 << 13;
        const CROSS    = 1 << 14;
        const SQUARE   = 1 << 15;
    }
}

/// Digital controller with 16 buttons (the first Sony controller).
/// It has no sticks.
pub struct DigitalController {
    state: State,

    poll_buttons: ButtonPressedCallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Command,
    Id,
    ButtonsLow,
    ButtonsHigh,
}

impl Default for DigitalController {
    fn default() -> Self {
        Self {
            state: State::Idle,
            poll_buttons: Box::new(Button::empty),
        }
    }
}

impl DigitalController {
    const ID_DIGITAL: u8 = 0x41;

    pub fn with_poll_buttons(poll_buttons: ButtonPressedCallback) -> Self {
        Self {
            poll_buttons,
            ..Default::default()
        }
    }
}

impl SerialDevice for DigitalController {
    fn select(&mut self) {
        self.state = State::Idle;
    }

    fn deselect(&mut self) {
        self.state = State::Idle;
    }

    fn exchange(&mut self, tx: u8) -> u8 {
        let mut pressed = || !(self.poll_buttons)().bits();

        match self.state {
            State::Idle => {
                if tx == 0x01 {
                    self.state = State::Command;
                }
                0xFF
            }

            State::Command => {
                if tx == 0x42 {
                    self.state = State::Id;
                    Self::ID_DIGITAL
                } else {
                    self.state = State::Idle;
                    0xFF
                }
            }

            State::Id => {
                self.state = State::ButtonsLow;
                0x5A
            }

            State::ButtonsLow => {
                self.state = State::ButtonsHigh;
                pressed() as u8
            }

            State::ButtonsHigh => {
                self.state = State::Idle;
                (pressed() >> 8) as u8
            }
        }
    }

    fn reset(&mut self) {
        let old = mem::take(self);
        self.poll_buttons = old.poll_buttons;
    }
}
