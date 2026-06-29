use smallbox::{SmallBox, space::S1};

use super::{
    CDROM_SECOND_DELAY, CDROM_SEEK_DELAY, CdRom, CdRomMode, CdRomStatus, ErrorCode, IrqFlag,
    bin_to_bcd,
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

pub struct Test {
    pub subcommand: u8,
}

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

pub struct Setmode {
    pub mode: u8,
}

impl Task for Setmode {
    fn busy_flag(&self) -> bool {
        true
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        cdrom.mode = CdRomMode::from_bits_truncate(self.mode);

        cdrom.push_response(&[cdrom.status.bits()]);
        cdrom.raise_int(IrqFlag::Int3);
    }
}

pub struct Setloc {
    pub mm: u8,
    pub ss: u8,
    pub ff: u8,
}

impl Task for Setloc {
    fn busy_flag(&self) -> bool {
        true
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        cdrom.msf_loc = Some([self.mm, self.ss, self.ff]);

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

pub struct Read;

impl Task for Read {
    fn busy_flag(&self) -> bool {
        true
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        cdrom.apply_setloc();

        cdrom.status.insert(CdRomStatus::READING);
        cdrom
            .status
            .remove(CdRomStatus::SEEKING | CdRomStatus::PLAYING);

        cdrom.pending_sector = None;
        cdrom.data_fifo.clear();
        cdrom.read_second_delivery_attempt = false;

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
            cdrom.read_second_delivery_attempt = false;
            return;
        }

        if cdrom.pending_sector.is_some() && !cdrom.read_second_delivery_attempt {
            cdrom.read_second_delivery_attempt = true;

            tracing::warn!(
                next_lba = cdrom.cursor_lba,
                "cdrom sector pending, giving CPU second chance"
            );

            cdrom.schedule_task(cdrom.read_sector_delay(), SmallBox::new(SectorReady));
            return;
        }

        if cdrom.pending_sector.is_some() {
            tracing::warn!(
                next_lba = cdrom.cursor_lba,
                "cdrom sector overrun: dropping previous pending sector"
            );

            cdrom.pending_sector = None;
            cdrom.read_second_delivery_attempt = false;
        }

        let Some(disc) = cdrom.disc.as_mut() else {
            cdrom.status.remove(CdRomStatus::READING);
            cdrom.read_second_delivery_attempt = false;
            cdrom.raise_err(ErrorCode::NoDisc);
            return;
        };

        let Some(raw_sector) = disc.read_sector(cdrom.cursor_lba) else {
            cdrom.status.remove(CdRomStatus::READING);
            cdrom.read_second_delivery_attempt = false;
            cdrom.raise_err(ErrorCode::BadParameter);
            return;
        };

        cdrom.cursor_lba = cdrom.cursor_lba.wrapping_add(1);

        if is_xa_audio_sector(cdrom, &raw_sector) {
            cdrom.schedule_task(cdrom.read_sector_delay(), SmallBox::new(SectorReady));

            return;
        }

        cdrom.pending_sector = Some(raw_sector);
        cdrom.read_second_delivery_attempt = false;

        cdrom.push_response(&[cdrom.status.bits()]);
        cdrom.raise_int(IrqFlag::Int1);

        cdrom.schedule_task(cdrom.read_sector_delay(), SmallBox::new(SectorReady));
    }
}

pub struct PauseFirst;

impl Task for PauseFirst {
    fn busy_flag(&self) -> bool {
        true
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        cdrom
            .status
            .remove(CdRomStatus::READING | CdRomStatus::PLAYING);

        cdrom.pending_sector = None;
        cdrom.data_fifo.clear();

        cdrom.push_response(&[cdrom.status.bits()]);
        cdrom.raise_int(IrqFlag::Int3);

        cdrom.schedule_task(CDROM_SECOND_DELAY, SmallBox::new(PauseSecond));
    }
}

pub struct PauseSecond;

impl Task for PauseSecond {
    fn busy_flag(&self) -> bool {
        false
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        cdrom.push_response(&[cdrom.status.bits()]);
        cdrom.raise_int(IrqFlag::Int2);
    }
}

pub struct Mute;

impl Task for Mute {
    fn busy_flag(&self) -> bool {
        true
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        cdrom.mute = true;

        cdrom.push_response(&[cdrom.status.bits()]);
        cdrom.raise_int(IrqFlag::Int3);
    }
}

pub struct Demute;

impl Task for Demute {
    fn busy_flag(&self) -> bool {
        true
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        cdrom.mute = false;

        cdrom.push_response(&[cdrom.status.bits()]);
        cdrom.raise_int(IrqFlag::Int3);
    }
}

pub struct GetTn;

impl Task for GetTn {
    fn busy_flag(&self) -> bool {
        true
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        // Single data track fallback.
        cdrom.push_response(&[
            cdrom.status.bits(),
            0x01, // first track, BCD
            0x01, // last track, BCD
        ]);

        cdrom.raise_int(IrqFlag::Int3);
    }
}

pub struct GetTd {
    pub track: u8,
}

impl Task for GetTd {
    fn busy_flag(&self) -> bool {
        true
    }

    fn execute(&mut self, cdrom: &mut CdRom) {
        let track = ((self.track >> 4) * 10) + (self.track & 0x0F);

        let (minutes, seconds) = match track {
            // track 0 = total disc length.
            0 => {
                let sectors = cdrom
                    .disc
                    .as_ref()
                    .map(|disc| disc.sector_count())
                    .unwrap_or(0);

                let total_seconds = sectors / 75;
                ((total_seconds / 60) as u8, (total_seconds % 60) as u8)
            }
            // track 1 starts at 00:02:00 in absolute MSF.
            1 => (0, 2),
            _ => {
                cdrom.raise_err(ErrorCode::BadParameter);
                return;
            }
        };

        cdrom.push_response(&[
            cdrom.status.bits(),
            bin_to_bcd(minutes),
            bin_to_bcd(seconds),
        ]);

        cdrom.raise_int(IrqFlag::Int3);
    }
}

fn is_xa_audio_sector(cdrom: &CdRom, raw: &[u8]) -> bool {
    let sector_mode = raw[3];
    let submode = raw[6];

    let audio = submode & 0x04 != 0;
    let realtime = submode & 0x40 != 0;

    sector_mode == 2 && cdrom.mode.contains(CdRomMode::XA_ADPCM) && audio && realtime
}
