//! ```text
//! CD-ROM Controller I/O Ports: 1F801800h..1F801803h
//!
//! 1F801800h write:
//!   bit0..1 = index for ports 1F801801h..1F801803h
//!   bit2..7 = ignored
//!
//! 1F801800h read:
//!   bit0..1 = current index
//!   bit2    = ADPBUSY  XA-ADPCM FIFO busy
//!   bit3    = PRMEMPT  parameter FIFO empty       (1 = empty)
//!   bit4    = PRMWRDY  parameter FIFO write ready (0 = full)
//!   bit5    = RSLRRDY  response FIFO has data     (1 = not empty)
//!   bit6    = DRQSTS   data FIFO has data / DRQ   (1 = not empty)
//!   bit7    = BUSYSTS  command busy               (1 = busy)
//!
//! +------------+-------+------------------------------+--------------------------------------+
//! | Address    | Index | Read                         | Write                                |
//! +------------+-------+------------------------------+--------------------------------------+
//! | 1F801800h  | any   | Index / Status               | Index select                         |
//! +------------+-------+------------------------------+--------------------------------------+
//! | 1F801801h  | 0     | Response FIFO                | Command                              |
//! | 1F801801h  | 1     | Response FIFO                | Sound map data out                   |
//! | 1F801801h  | 2     | Response FIFO                | Sound map coding info                |
//! | 1F801801h  | 3     | Response FIFO                | Audio volume right-CD -> right-SPU   |
//! +------------+-------+------------------------------+--------------------------------------+
//! | 1F801802h  | 0     | Data FIFO                    | Parameter FIFO                       |
//! | 1F801802h  | 1     | Data FIFO                    | Interrupt enable                     |
//! | 1F801802h  | 2     | Data FIFO                    | Audio volume left-CD -> left-SPU     |
//! | 1F801802h  | 3     | Data FIFO                    | Audio volume right-CD -> left-SPU    |
//! +------------+-------+------------------------------+--------------------------------------+
//! | 1F801803h  | 0     | Interrupt enable             | Request                              |
//! | 1F801803h  | 1     | Interrupt flag               | Interrupt ack / param FIFO reset     |
//! | 1F801803h  | 2     | Interrupt enable mirror      | Audio volume left-CD -> right-SPU    |
//! | 1F801803h  | 3     | Interrupt flag mirror        | Audio volume apply / mute            |
//! +------------+-------+------------------------------+--------------------------------------+
//! ```
//! Source: [PSX-SPX CDROM Controller I/O Ports](https://problemkaputt.de/psxspx-cdrom-controller-i-o-ports.htm).
use alloc::collections::vec_deque::VecDeque;

use modular_bitfield::*;

use crate::devices::int::{InterruptController, InterruptFlags};

use super::Mmio;

mod cmd;

const PARAM_FIFO_CAP: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CdRomInt {
    /// DataReady
    Int1 = 1,
    /// Complete / second response
    Int2 = 2,
    /// Acknowledge / first response
    Int3 = 3,
    /// DataEnd
    Int4 = 4,
    /// Error
    Int5 = 5,
}

pub struct CdRom {
    index: Index,

    param_fifo: VecDeque<u8>,
    response_fifo: VecDeque<u8>,
    data_fifo: VecDeque<u8>,

    irq_enable: u8,
    irq_flags: u8,
    irq_check_pending: bool,

    busy: bool,

    volume_cd_left_to_spu_left: u8,
    volume_cd_left_to_spu_right: u8,
    volume_cd_right_to_spu_left: u8,
    volume_cd_right_to_spu_right: u8,
}

#[bitfield(bits = 8)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Status {
    /// Current register index.
    index: Index,
    /// XA-ADPCM decoder busy.
    adpcm_busy: bool,
    /// Parameter FIFO empty.
    parameter_fifo_empty: bool,
    /// Parameter FIFO can accept writes.
    parameter_fifo_write_ready: bool,
    /// Response FIFO has data.
    response_fifo_not_empty: bool,
    /// Data FIFO has data / DRQ.
    data_fifo_not_empty: bool,
    /// Command busy.
    busy: bool,
}

#[derive(Specifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 2]
enum Index {
    Zero = 0,
    First = 1,
    Second = 2,
    Third = 3,
}

impl Default for CdRom {
    fn default() -> Self {
        Self {
            index: Index::Zero,

            param_fifo: VecDeque::with_capacity(PARAM_FIFO_CAP),
            response_fifo: VecDeque::new(),
            data_fifo: VecDeque::new(),

            irq_enable: 0,
            irq_flags: 0,
            irq_check_pending: false,

            busy: false,

            volume_cd_left_to_spu_left: 0,
            volume_cd_left_to_spu_right: 0,
            volume_cd_right_to_spu_left: 0,
            volume_cd_right_to_spu_right: 0,
        }
    }
}

impl CdRom {
    pub fn update(&mut self, int_ctrl: &mut InterruptController) {
        if self.irq_check_pending && self.irq_enable & self.irq_flags != 0 {
            int_ctrl.raise(InterruptFlags::CDROM);
            self.irq_check_pending = false;
        }
    }

    fn stat(&self) -> Status {
        Status::new()
            .with_index(self.index)
            .with_adpcm_busy(false)
            .with_parameter_fifo_empty(self.param_fifo.is_empty())
            .with_parameter_fifo_write_ready(self.param_fifo.len() < PARAM_FIFO_CAP)
            .with_response_fifo_not_empty(!self.response_fifo.is_empty())
            .with_data_fifo_not_empty(!self.data_fifo.is_empty())
            .with_busy(self.busy)
    }

    fn raise_int(&mut self, int: CdRomInt) {
        self.irq_flags = int as u8;
        self.irq_check_pending = true;
    }
}

impl Mmio for CdRom {
    fn read(&mut self, dest: &mut [u8], maddr: u32) {
        assert_eq!(dest.len(), 1, "only 1-byte access supported");

        dest[0] = match maddr & 3 {
            0x0 => {
                let [stat] = self.stat().into_bytes();
                stat
            }

            0x1 => self.response_fifo.pop_front().unwrap_or_default(),

            0x2 => self.data_fifo.pop_front().unwrap_or_default(),

            0x3 => match self.index {
                // write-only reg
                Index::Zero | Index::Second => (self.irq_enable & 0x1F) | 0xE0,
                // same shit logic as above, 5 bits for [1; 5]
                Index::First | Index::Third => (self.irq_flags & 0x1F) | 0xE0,
            },

            _ => unimplemented!(),
        };
    }

    fn write(&mut self, maddr: u32, value: &[u8]) {
        let Ok([value]) = <[_; 1]>::try_from(value) else {
            panic!("only 1-byte access supported");
        };

        match maddr & 3 {
            0x0 => {
                self.index = match value & 0x3 {
                    0 => Index::Zero,
                    1 => Index::First,
                    2 => Index::Second,
                    3 => Index::Third,
                    _ => unreachable!(),
                };
            }

            0x1 => match self.index {
                Index::Zero => self.command(value),
                // TODO : Audio
                // Index::First => self.sound_map_data_out = value,
                // Index::Second => self.sound_map_coding_info = value,
                Index::Third => self.volume_cd_right_to_spu_right = value,
                _ => {}
            },

            0x2 => match self.index {
                Index::Zero => {
                    if self.param_fifo.len() < PARAM_FIFO_CAP {
                        self.param_fifo.push_back(value);
                    } else {
                        tracing::warn!(cap=%PARAM_FIFO_CAP, "cdrom parameter fifo overflow");
                        self.raise_int(CdRomInt::Int5);
                    }
                }
                Index::First => {
                    self.irq_enable = value & 0x1F;
                    self.irq_check_pending = true;
                }
                Index::Second => self.volume_cd_left_to_spu_left = value,
                Index::Third => self.volume_cd_right_to_spu_left = value,
            },

            0x3 => match self.index {
                Index::Zero => {
                    // bit 7: BFRD / Want Data
                    if value & 0x80 != 0 {
                        // self.load_pending_sector_to_data_fifo();
                    } else {
                        self.data_fifo.clear();
                    }
                }
                Index::First => {
                    // W1C behavior
                    self.irq_flags &= !(value & 0x1F);
                    self.irq_check_pending = true;

                    if value & 0x40 != 0 {
                        self.param_fifo.clear();
                    }
                }
                Index::Second => self.volume_cd_left_to_spu_right = value,
                // TODO : Audio
                // Index::Third => self.volume_apply(value),
                _ => {}
            },

            _ => unreachable!(),
        }
    }
}
