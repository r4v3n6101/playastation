use smallbox::{SmallBox, space::S1};

use super::{
    CDROM_SECOND_DELAY, CDROM_SEEK_DELAY, CdRom, CdRomMode, CdRomStatus, ErrorCode, IrqFlag,
};

pub type BoxedTask = SmallBox<dyn Task, S1>;

pub trait Task {
    fn busy_flag(&self) -> bool;
    fn execute(&mut self, cdrom: &mut CdRom);
}

pub struct ScheduledTask {
    pub sys_cycles_left: u64,
    pub task: BoxedTask,
}

pub struct BadCommand {
    pub cmd: u8,
}

impl Task for BadCommand {
    fn busy_flag(&self) -> bool {
        true
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        tracing::warn!(cmd=%format_args!("{:#X}", self.cmd), "bad cdrom command");

        cdrom.raise_err(ErrorCode::BadCommand);
    }
}

pub struct Test;

impl Task for Test {
    fn busy_flag(&self) -> bool {
        true
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        cdrom.push_response(&[0x94, 0x09, 0x19, 0xC0]);
        cdrom.raise_int(IrqFlag::Int3);
    }
}

pub struct Getstat;

impl Task for Getstat {
    fn busy_flag(&self) -> bool {
        true
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        cdrom.push_response(&[cdrom.status.bits()]);
        cdrom.raise_int(IrqFlag::Int3);
    }
}

pub struct InitFirst;

impl Task for InitFirst {
    fn busy_flag(&self) -> bool {
        true
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        cdrom.mode = CdRomMode::empty();

        cdrom.status.remove(
            CdRomStatus::READING | CdRomStatus::SEEKING | CdRomStatus::PLAYING | CdRomStatus::ERROR,
        );

        cdrom.data_fifo.clear();
        cdrom.pending_sector = None;

        cdrom.push_response(&[cdrom.status.bits()]);
        cdrom.raise_int(IrqFlag::Int3);

        cdrom.schedule_task(CDROM_SECOND_DELAY, SmallBox::new(InitSecond));
    }
}

pub struct InitSecond;

impl Task for InitSecond {
    fn busy_flag(&self) -> bool {
        false
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        cdrom.push_response(&[cdrom.status.bits()]);
        cdrom.raise_int(IrqFlag::Int2);
    }
}

pub struct GetIdFirst;

impl Task for GetIdFirst {
    fn busy_flag(&self) -> bool {
        true
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        cdrom.push_response(&[cdrom.status.bits()]);
        cdrom.raise_int(IrqFlag::Int3);

        cdrom.schedule_task(CDROM_SECOND_DELAY, SmallBox::new(GetIdSecond));
    }
}

pub struct GetIdSecond;

impl Task for GetIdSecond {
    fn busy_flag(&self) -> bool {
        false
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        // Eurotour!
        cdrom.push_response(&[0x02, 0x00, 0x20, 0x00, b'S', b'C', b'E', b'E']);
        cdrom.raise_int(IrqFlag::Int2);
    }
}

pub struct Setmode;

impl Task for Setmode {
    fn busy_flag(&self) -> bool {
        true
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        let Some(mode) = cdrom.param_fifo.pop_front() else {
            cdrom.raise_err(ErrorCode::BadParameter);
            return;
        };

        cdrom.mode = CdRomMode::from_bits_truncate(mode);

        cdrom.push_response(&[cdrom.status.bits()]);
        cdrom.raise_int(IrqFlag::Int3);
    }
}

pub struct Setloc;

impl Task for Setloc {
    fn busy_flag(&self) -> bool {
        true
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        let Some(mm) = cdrom.param_fifo.pop_front() else {
            cdrom.raise_err(ErrorCode::BadParameter);
            return;
        };
        let Some(ss) = cdrom.param_fifo.pop_front() else {
            cdrom.raise_err(ErrorCode::BadParameter);
            return;
        };
        let Some(ff) = cdrom.param_fifo.pop_front() else {
            cdrom.raise_err(ErrorCode::BadParameter);
            return;
        };

        cdrom.msf_loc = Some([mm, ss, ff]);

        cdrom.push_response(&[cdrom.status.bits()]);
        cdrom.raise_int(IrqFlag::Int3);
    }
}

pub struct SeekFirst;

impl Task for SeekFirst {
    fn busy_flag(&self) -> bool {
        true
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        cdrom.apply_setloc();

        cdrom.status.insert(CdRomStatus::SEEKING);
        cdrom
            .status
            .remove(CdRomStatus::READING | CdRomStatus::PLAYING);

        cdrom.push_response(&[cdrom.status.bits()]);
        cdrom.raise_int(IrqFlag::Int3);

        cdrom.schedule_task(CDROM_SEEK_DELAY, SmallBox::new(SeekSecond));
    }
}

pub struct SeekSecond;

impl Task for SeekSecond {
    fn busy_flag(&self) -> bool {
        false
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        cdrom.status.remove(CdRomStatus::SEEKING);

        cdrom.push_response(&[cdrom.status.bits()]);
        cdrom.raise_int(IrqFlag::Int2);
    }
}

pub struct ReadN;

impl Task for ReadN {
    fn busy_flag(&self) -> bool {
        true
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        cdrom.apply_setloc();

        cdrom.status.insert(CdRomStatus::READING);
        cdrom
            .status
            .remove(CdRomStatus::SEEKING | CdRomStatus::PLAYING);

        cdrom.push_response(&[cdrom.status.bits()]);
        cdrom.raise_int(IrqFlag::Int3);

        cdrom.schedule_task(cdrom.read_sector_delay(), SmallBox::new(SectorReady));
    }
}

pub struct SectorReady;

impl Task for SectorReady {
    fn busy_flag(&self) -> bool {
        false
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        if !cdrom.status.contains(CdRomStatus::READING) {
            return;
        }

        if cdrom.pending_sector.is_some() {
            tracing::warn!("cdrom sector overrun: previous sector still pending");
        } else {
            let Some(disc) = cdrom.disc.as_mut() else {
                cdrom.status.remove(CdRomStatus::READING);
                cdrom.raise_err(ErrorCode::NoDisc);
                return;
            };

            let Some(raw_sector) = disc.read_sector(cdrom.cursor_lba) else {
                cdrom.status.remove(CdRomStatus::READING);
                cdrom.raise_err(ErrorCode::BadParameter);
                return;
            };

            cdrom.cursor_lba = cdrom.cursor_lba.wrapping_add(1);
            cdrom.pending_sector = Some(raw_sector);

            cdrom.push_response(&[cdrom.status.bits()]);
            cdrom.raise_int(IrqFlag::Int1);
        }

        cdrom.schedule_task(cdrom.read_sector_delay(), SmallBox::new(SectorReady));
    }
}
