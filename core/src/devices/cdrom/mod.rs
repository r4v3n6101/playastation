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
use alloc::{boxed::Box, collections::VecDeque};

use modular_bitfield::prelude::*;

use crate::{
    SystemTime,
    devices::int::{InterruptController, InterruptFlags},
    formats::disk::{Disc, RawSector, sector_data},
    interconnect::scheduler::{Event, Scheduler},
};

mod inner;

const PARAM_FIFO_CAP: usize = 16;

const CDROM_COMMAND_DEFAULT_DELAY: SystemTime = 0x1100;
const CDROM_SECOND_DELAY: SystemTime = 0x3000;
const CDROM_SEEK_DELAY: SystemTime = 0x30000;
// https://github.com/Amjad50/Trapezoid/blob/b2411afe405a4c1d33586338b0768b3343f8353f/trapezoid-core/src/cdrom.rs#L22
const CDROM_READ_PLAY_DELAY: SystemTime = 0x6e400 - 0x100;

bitflags::bitflags! {
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct CdRomStatus: u8 {
        const ERROR      = 1 << 0;
        const MOTOR_ON   = 1 << 1;
        const SEEK_ERROR = 1 << 2;
        const ID_ERROR   = 1 << 3;
        const SHELL_OPEN = 1 << 4;
        const READING    = 1 << 5;
        const SEEKING    = 1 << 6;
        const PLAYING    = 1 << 7;
    }
}

bitflags::bitflags! {
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct CdRomMode: u8 {
        const CDDA         = 1 << 0;
        const AUTOPAUSE    = 1 << 1;
        const REPORT_INTS  = 1 << 2;
        const XA_FILTER    = 1 << 3;
        const IGNORE_BIT   = 1 << 4;
        const WHOLE_SECTOR = 1 << 5;
        const XA_ADPCM     = 1 << 6;
        const DOUBLE_SPEED = 1 << 7;
    }
}

pub struct CdRom {
    pub disc: Option<Box<dyn Disc>>,
    pub status: CdRomStatus,
    pub mode: CdRomMode,
    pub mute: bool,

    index: BankIndex,

    param_fifo: VecDeque<u8>,
    response_fifo: VecDeque<u8>,
    data_fifo: VecDeque<u8>,

    pending: VecDeque<inner::ScheduledCommand>,

    irq_enable: u8,
    irq_flags: u8,

    cursor_lba: usize,
    msf_loc: Option<[u8; 3]>,
    filter_file: u8,
    filter_channel: u8,
    pending_sector: Option<RawSector>,
    read_second_delivery_attempt: bool,

    volume_cd_left_to_spu_left: u8,
    volume_cd_left_to_spu_right: u8,
    volume_cd_right_to_spu_left: u8,
    volume_cd_right_to_spu_right: u8,
}

#[bitfield(bits = 8)]
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct CdRomStat {
    /// Current register index.
    pub index: BankIndex,
    /// XA-ADPCM decoder busy.
    pub adpcm_busy: bool,
    /// Parameter FIFO empty.
    pub parameter_fifo_empty: bool,
    /// Parameter FIFO can accept writes.
    pub parameter_fifo_write_ready: bool,
    /// Response FIFO has data.
    pub response_fifo_not_empty: bool,
    /// Data FIFO has data / DRQ.
    pub data_fifo_not_empty: bool,
    /// Command busy.
    pub busy: bool,
}

#[derive(Specifier, Debug, Copy, Clone, PartialEq, Eq)]
#[bits = 2]
pub enum BankIndex {
    Zero = 0,
    First = 1,
    Second = 2,
    Third = 3,
}

#[derive(Copy, Clone)]
enum IrqFlag {
    /// `DataReady`
    Int1 = 1,
    /// Complete / second response
    Int2 = 2,
    /// Acknowledge / first response
    Int3 = 3,
    /// `DataEnd`
    Int4 = 4,
    /// Error
    Int5 = 5,
}

#[repr(u8)]
enum ErrorCode {
    BadSubFunction = 0x10,
    BadParameter = 0x20,
    BadCommand = 0x40,
    NoDisc = 0x80,
}

impl Default for CdRom {
    fn default() -> Self {
        Self {
            disc: None,
            status: CdRomStatus::MOTOR_ON,
            mode: CdRomMode::empty(),
            mute: false,

            index: BankIndex::Zero,

            param_fifo: VecDeque::with_capacity(PARAM_FIFO_CAP),
            response_fifo: VecDeque::new(),
            data_fifo: VecDeque::new(),

            pending: VecDeque::new(),

            irq_enable: 0,
            irq_flags: 0,

            cursor_lba: 0,
            msf_loc: None,
            filter_file: 0,
            filter_channel: 0,
            pending_sector: None,
            read_second_delivery_attempt: false,

            volume_cd_left_to_spu_left: 0,
            volume_cd_left_to_spu_right: 0,
            volume_cd_right_to_spu_left: 0,
            volume_cd_right_to_spu_right: 0,
        }
    }
}

impl CdRom {
    pub fn stat(&self) -> CdRomStat {
        let busy = self
            .pending
            .iter()
            .any(|scheduled| scheduled.cmd.busy_flag());

        CdRomStat::new()
            .with_index(self.index)
            .with_adpcm_busy(false)
            .with_parameter_fifo_empty(self.param_fifo.is_empty())
            .with_parameter_fifo_write_ready(self.param_fifo.len() < PARAM_FIFO_CAP)
            .with_response_fifo_not_empty(!self.response_fifo.is_empty())
            .with_data_fifo_not_empty(!self.data_fifo.is_empty())
            .with_busy(busy)
    }

    pub(crate) fn pop_data(&mut self) -> Option<u8> {
        self.data_fifo.pop_front()
    }

    pub(crate) fn update(&mut self, scheduler: &mut Scheduler, int_ctrl: &mut InterruptController) {
        let elapsed = scheduler.take_elapsed(Event::CdRom);

        for scheduled in &mut self.pending {
            scheduled.remaining_delay = scheduled.remaining_delay.saturating_sub(elapsed);
        }

        while self.irq_flags == 0
            && self
                .pending
                .front()
                .is_some_and(|scheduled| scheduled.remaining_delay == 0)
        {
            let task = self.pending.pop_front().unwrap().cmd;
            task.execute(self, int_ctrl);
        }

        scheduler.remove(Event::CdRom);
        if let Some(scheduled) = self.pending.front()
            && scheduled.remaining_delay != 0
        {
            scheduler.schedule(Event::CdRom, scheduled.remaining_delay);
        }
    }

    pub(crate) fn read_mmio(&mut self, dest: &mut [u8], maddr: u32) {
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
                BankIndex::Zero | BankIndex::Second => (self.irq_enable & 0x1F) | 0xE0,
                // same shit logic as above, 5 bits for [1; 5]
                BankIndex::First | BankIndex::Third => (self.irq_flags & 0x1F) | 0xE0,
            },
            _ => unimplemented!(),
        };
    }

    pub(crate) fn write_mmio(
        &mut self,
        scheduler: &mut Scheduler,
        int_ctrl: &mut InterruptController,
        maddr: u32,
        value: &[u8],
    ) {
        let Ok([value]) = <[_; 1]>::try_from(value) else {
            panic!("only 1-byte access supported");
        };

        match maddr & 3 {
            0x0 => {
                self.index = match value & 0x3 {
                    0 => BankIndex::Zero,
                    1 => BankIndex::First,
                    2 => BankIndex::Second,
                    3 => BankIndex::Third,
                    _ => unreachable!(),
                };
            }
            0x1 => match self.index {
                BankIndex::Zero => {
                    let cmd = match value {
                        0x01 => inner::Command::Getstat,
                        0x02 => {
                            let mm = self.param_fifo.pop_front().unwrap_or(0);
                            let ss = self.param_fifo.pop_front().unwrap_or(0);
                            let ff = self.param_fifo.pop_front().unwrap_or(0);
                            inner::Command::Setloc { mm, ss, ff }
                        }
                        0x06 | 0x1B => inner::Command::Read,
                        0x09 => inner::Command::PauseFirst,
                        0x0A => inner::Command::InitFirst,
                        0x0B => inner::Command::Mute,
                        0x0C => inner::Command::Demute,
                        0x0D => {
                            let file = self.param_fifo.pop_front().unwrap_or(0);
                            let channel = self.param_fifo.pop_front().unwrap_or(0);
                            inner::Command::Setfilter { file, channel }
                        }
                        0x0E => {
                            let mode = self.param_fifo.pop_front().unwrap_or(0);
                            inner::Command::Setmode { mode }
                        }
                        0x13 => inner::Command::GetTn,
                        0x14 => {
                            let track = self.param_fifo.pop_front().unwrap_or(0);
                            inner::Command::GetTd { track }
                        }
                        0x15 | 0x16 => inner::Command::SeekFirst,
                        0x19 => {
                            let subcommand = self.param_fifo.pop_front().unwrap_or(0);
                            inner::Command::Test { subcommand }
                        }
                        0x1A => inner::Command::GetIdFirst,
                        cmd => inner::Command::Bad { cmd },
                    };
                    self.param_fifo.clear();

                    let remaining_delay = scheduler
                        .elapsed_since_update(Event::CdRom)
                        .checked_add(CDROM_COMMAND_DEFAULT_DELAY)
                        .expect("cdrom task deadline overflow");

                    self.queue_task(cmd, remaining_delay);
                    scheduler.schedule(Event::CdRom, CDROM_COMMAND_DEFAULT_DELAY);
                }
                // TODO : Audio
                // Index::First => self.sound_map_data_out = value,
                // Index::Second => self.sound_map_coding_info = value,
                BankIndex::Third => self.volume_cd_right_to_spu_right = value,
                _ => {}
            },
            0x2 => match self.index {
                BankIndex::Zero => {
                    if self.param_fifo.len() < PARAM_FIFO_CAP {
                        self.param_fifo.push_back(value);
                    } else {
                        tracing::warn!(cap=%PARAM_FIFO_CAP, "cdrom parameter fifo overflow");
                        self.raise_int(int_ctrl, IrqFlag::Int5);
                    }
                }
                BankIndex::First => {
                    let was_asserted = self.irq_signal();
                    self.irq_enable = value & 0x1F;
                    if !was_asserted && self.irq_signal() {
                        int_ctrl.raise(InterruptFlags::CDROM);
                    }
                }
                BankIndex::Second => self.volume_cd_left_to_spu_left = value,
                BankIndex::Third => self.volume_cd_right_to_spu_left = value,
            },
            0x3 => match self.index {
                BankIndex::Zero => {
                    // bit 7: BFRD / Want Data
                    if value & 0x80 != 0 {
                        let Some(data) = self.pending_sector.take() else {
                            return;
                        };

                        self.data_fifo.clear();
                        if self.mode.contains(CdRomMode::WHOLE_SECTOR) {
                            self.data_fifo.extend(data);
                        } else {
                            self.data_fifo.extend(sector_data(&data));
                        }
                    } else if self.pending_sector.is_some() {
                        self.data_fifo.clear();
                    }
                }
                BankIndex::First => {
                    // W1C behavior
                    self.irq_flags &= !(value & 0x1F);

                    if value & 0x40 != 0 {
                        self.param_fifo.clear();
                    }

                    scheduler.schedule(Event::CdRom, 0);
                }
                BankIndex::Second => self.volume_cd_left_to_spu_right = value,
                // TODO : Audio
                // Index::Third => self.volume_apply(value),
                BankIndex::Third => {}
            },
            _ => unreachable!(),
        }
    }

    fn queue_task(&mut self, task: inner::Command, remaining_delay: SystemTime) {
        let index = self
            .pending
            .iter()
            .position(|scheduled| scheduled.remaining_delay > remaining_delay)
            .unwrap_or(self.pending.len());
        self.pending.insert(
            index,
            inner::ScheduledCommand {
                remaining_delay,
                cmd: task,
            },
        );
    }

    fn cancel_read(&mut self) {
        self.pending
            .retain(|scheduled| scheduled.cmd != inner::Command::SectorReady);
        self.read_second_delivery_attempt = false;
    }

    fn read_sector_delay(&self) -> SystemTime {
        if self.mode.contains(CdRomMode::DOUBLE_SPEED) {
            CDROM_READ_PLAY_DELAY / 2
        } else {
            CDROM_READ_PLAY_DELAY
        }
    }

    fn apply_setloc(&mut self) {
        let Some([mm, ss, ff]) = self.msf_loc.take() else {
            return;
        };

        let minutes = bcd_to_bin(mm) as i32;
        let seconds = bcd_to_bin(ss) as i32;
        let frames = bcd_to_bin(ff) as i32;

        let lba = ((minutes * 60) + seconds) * 75 + frames - 150;

        self.cursor_lba = lba.max(0) as usize;
    }

    fn raise_err(&mut self, int_ctrl: &mut InterruptController, err: ErrorCode) {
        self.push_response(&[(self.status | CdRomStatus::ERROR).bits(), err as u8]);
        self.raise_int(int_ctrl, IrqFlag::Int5);
    }

    fn push_response(&mut self, data: &[u8]) {
        self.response_fifo.clear();
        self.response_fifo.extend(data);
    }

    fn raise_int(&mut self, int_ctrl: &mut InterruptController, int: IrqFlag) {
        let was_asserted = self.irq_signal();
        self.irq_flags = int as u8;
        if !was_asserted && self.irq_signal() {
            int_ctrl.raise(InterruptFlags::CDROM);
        }
    }

    fn irq_signal(&self) -> bool {
        self.irq_enable & self.irq_flags != 0
    }
}

fn bcd_to_bin(value: u8) -> u8 {
    ((value >> 4) * 10) + (value & 0x0F)
}

fn bin_to_bcd(value: u8) -> u8 {
    ((value / 10) << 4) | (value % 10)
}
