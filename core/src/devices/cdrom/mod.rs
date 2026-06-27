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
use smallbox::{SmallBox, space::S2};

use crate::devices::int::{InterruptController, InterruptFlags};

use super::Mmio;

mod tasks;

const PARAM_FIFO_CAP: usize = 16;

const CDROM_COMMAND_DEFAULT_DELAY: u64 = 0x1100;
/// https://github.com/Amjad50/Trapezoid/blob/b2411afe405a4c1d33586338b0768b3343f8353f/trapezoid-core/src/cdrom.rs#L22
const CDROM_READ_PLAY_DELAY: u64 = 0x6E400 - 0x100;

type BoxedTask = SmallBox<dyn Task, S2>;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Copy, Clone)]
enum IrqFlag {
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

#[repr(u8)]
enum ErrorCode {
    BadSubFunction = 0x10,
    BadParameter = 0x20,
    BadCommand = 0x40,
    NoDisc = 0x80,
}

trait Task {
    fn busy_flag(&self) -> bool;
    fn execute(&mut self, cdrom: &mut CdRom);
}

pub struct CdRom {
    pub status: CdRomStatus,

    index: RegisterIndex,

    param_fifo: VecDeque<u8>,
    response_fifo: VecDeque<u8>,
    data_fifo: VecDeque<u8>,

    scheduled_tasks: VecDeque<TaskEntry>,
    pending_task: Option<BoxedTask>,

    irq_enable: u8,
    irq_flags: u8,

    volume_cd_left_to_spu_left: u8,
    volume_cd_left_to_spu_right: u8,
    volume_cd_right_to_spu_left: u8,
    volume_cd_right_to_spu_right: u8,
}

#[bitfield(bits = 8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct CdRomStat {
    /// Current register index.
    pub index: RegisterIndex,
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
pub enum RegisterIndex {
    Zero = 0,
    First = 1,
    Second = 2,
    Third = 3,
}

struct TaskEntry {
    sys_cycles_left: u64,
    task: BoxedTask,
}

impl Default for CdRom {
    fn default() -> Self {
        Self {
            status: CdRomStatus::empty(),

            index: RegisterIndex::Zero,

            param_fifo: VecDeque::with_capacity(PARAM_FIFO_CAP),
            response_fifo: VecDeque::new(),
            data_fifo: VecDeque::new(),

            scheduled_tasks: VecDeque::new(),
            pending_task: None,

            irq_enable: 0,
            irq_flags: 0,

            volume_cd_left_to_spu_left: 0,
            volume_cd_left_to_spu_right: 0,
            volume_cd_right_to_spu_left: 0,
            volume_cd_right_to_spu_right: 0,
        }
    }
}

impl CdRom {
    pub fn stat(&self) -> CdRomStat {
        let pending_tasks =
            self.pending_task.is_some() || self.scheduled_tasks.iter().any(|x| x.task.busy_flag());

        CdRomStat::new()
            .with_index(self.index)
            .with_adpcm_busy(false)
            .with_parameter_fifo_empty(self.param_fifo.is_empty())
            .with_parameter_fifo_write_ready(self.param_fifo.len() < PARAM_FIFO_CAP)
            .with_response_fifo_not_empty(!self.response_fifo.is_empty())
            .with_data_fifo_not_empty(!self.data_fifo.is_empty())
            .with_busy(pending_tasks)
    }

    pub(crate) fn update(&mut self, int_ctrl: &mut InterruptController, sys_cycles: u64) {
        self.tick_scheduled(sys_cycles);

        // Next task needs IRQ ack
        if self.irq_flags == 0 {
            self.handle_ready_task();
        }

        if self.irq_enable & self.irq_flags != 0 {
            int_ctrl.raise(InterruptFlags::CDROM);
        }
    }

    /// Create a delay before task can be executed.
    fn tick_scheduled(&mut self, cycles: u64) {
        if self.pending_task.is_some() {
            return;
        }

        let Some(entry) = self.scheduled_tasks.front_mut() else {
            return;
        };

        if entry.sys_cycles_left > cycles {
            entry.sys_cycles_left -= cycles;
            return;
        }

        let entry = self.scheduled_tasks.pop_front().unwrap();
        self.pending_task = Some(entry.task);
    }

    /// Handle command or other event in queue, but only if IRQ flag is not set.
    fn handle_ready_task(&mut self) {
        let Some(mut task) = self.pending_task.take() else {
            return;
        };

        task.execute(self);
    }

    fn raise_err(&mut self, err: ErrorCode) {
        self.push_response(&[(self.status | CdRomStatus::ERROR).bits(), err as u8]);
        self.raise_int(IrqFlag::Int5);
    }

    fn push_response(&mut self, data: &[u8]) {
        self.response_fifo.clear();
        self.response_fifo.extend(data);
    }

    fn raise_int(&mut self, int: IrqFlag) {
        self.irq_flags = int as u8;
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
                RegisterIndex::Zero | RegisterIndex::Second => (self.irq_enable & 0x1F) | 0xE0,
                // same shit logic as above, 5 bits for [1; 5]
                RegisterIndex::First | RegisterIndex::Third => (self.irq_flags & 0x1F) | 0xE0,
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
                    0 => RegisterIndex::Zero,
                    1 => RegisterIndex::First,
                    2 => RegisterIndex::Second,
                    3 => RegisterIndex::Third,
                    _ => unreachable!(),
                };
            }
            0x1 => match self.index {
                RegisterIndex::Zero => self.scheduled_tasks.push_back(TaskEntry {
                    sys_cycles_left: CDROM_COMMAND_DEFAULT_DELAY,
                    task: match value {
                        // 0x01 => Command::Getstat,
                        // 0x0A => Command::Init,
                        // 0x1A => Command::GetId,
                        // 0x0E => Command::Setmode,
                        // 0x02 => Command::Setloc,
                        // 0x15 => Command::SeekL,
                        // 0x16 => Command::SeekP,
                        // 0x06 => Command::ReadN,
                        // 0x09 => Command::Pause,
                        // 0x08 => Command::Stop,
                        // other => Command::Bad(other),
                        0x19 => SmallBox::new(tasks::Test),
                        other => SmallBox::new(tasks::BadCommand { command: other }),
                    },
                }),
                // TODO : Audio
                // Index::First => self.sound_map_data_out = value,
                // Index::Second => self.sound_map_coding_info = value,
                RegisterIndex::Third => self.volume_cd_right_to_spu_right = value,
                _ => {}
            },
            0x2 => match self.index {
                RegisterIndex::Zero => {
                    if self.param_fifo.len() < PARAM_FIFO_CAP {
                        self.param_fifo.push_back(value);
                    } else {
                        tracing::warn!(cap=%PARAM_FIFO_CAP, "cdrom parameter fifo overflow");
                        self.raise_int(IrqFlag::Int5);
                    }
                }
                RegisterIndex::First => {
                    self.irq_enable = value & 0x1F;
                }
                RegisterIndex::Second => self.volume_cd_left_to_spu_left = value,
                RegisterIndex::Third => self.volume_cd_right_to_spu_left = value,
            },
            0x3 => match self.index {
                RegisterIndex::Zero => {
                    // bit 7: BFRD / Want Data
                    if value & 0x80 != 0 {
                        // self.load_pending_sector_to_data_fifo();
                    } else {
                        self.data_fifo.clear();
                    }
                }
                RegisterIndex::First => {
                    // W1C behavior
                    self.irq_flags &= !(value & 0x1F);

                    if value & 0x40 != 0 {
                        self.param_fifo.clear();
                    }
                }
                RegisterIndex::Second => self.volume_cd_left_to_spu_right = value,
                // TODO : Audio
                // Index::Third => self.volume_apply(value),
                _ => {}
            },
            _ => unreachable!(),
        }
    }
}
