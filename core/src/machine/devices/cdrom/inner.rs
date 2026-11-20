use crate::{
    SystemTime,
    interfaces::disc::{RawSector, TrackMode},
    machine::devices::int::InterruptController,
};

use super::{
    CDROM_SECOND_DELAY, CDROM_SEEK_DELAY, CdRom, CdRomMode, CdRomStatus, ErrorCode, IrqFlag, Msf,
    bcd_to_bin, bin_to_bcd,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Bad { cmd: u8 },
    Test { subcommand: u8 },
    Getstat,
    InitFirst,
    InitSecond,
    GetIdFirst,
    GetIdSecond,
    Setfilter { file: u8, channel: u8 },
    Setmode { mode: u8 },
    Setloc { mm: u8, ss: u8, ff: u8 },
    SeekFirst,
    SeekSecond,
    Read,
    Play { track: u8 },
    SectorReady { retry: bool },
    StopFirst,
    StopSecond,
    PauseFirst,
    PauseSecond,
    Mute,
    Demute,
    GetTn,
    GetTd { track: u8 },
    GetlocP,
}

pub struct ScheduledCommand {
    pub remaining_delay: SystemTime,
    pub cmd: Command,
}

enum SectorStep {
    Advance,
    Retry,
    Stopped,
}

impl Command {
    pub fn busy_flag(self) -> bool {
        !matches!(
            self,
            Self::InitSecond
                | Self::GetIdSecond
                | Self::SeekSecond
                | Self::SectorReady { .. }
                | Self::PauseSecond
                | Self::StopSecond
        )
    }

    pub fn execute(self, cdrom: &mut CdRom, int_ctrl: &mut InterruptController) {
        if let Err(err) = execute_command(self, cdrom, int_ctrl) {
            cdrom.raise_err(int_ctrl, err);
        }
    }
}

fn execute_command(
    command: Command,
    cdrom: &mut CdRom,
    int_ctrl: &mut InterruptController,
) -> Result<(), ErrorCode> {
    match command {
        Command::Bad { cmd } => {
            tracing::warn!(cmd=%format_args!("{:#X}", cmd), "bad cdrom command");

            return Err(ErrorCode::BadCommand);
        }
        Command::Test { subcommand: _ } => {
            cdrom.push_response(&[0x94, 0x09, 0x19, 0xC0]);
            cdrom.raise_int(int_ctrl, IrqFlag::Int3);
        }
        Command::Getstat => {
            reply_status(cdrom, int_ctrl, IrqFlag::Int3);
        }
        Command::InitFirst => {
            clear_transfer(cdrom);

            cdrom.mode = CdRomMode::empty();
            cdrom.status.remove(
                CdRomStatus::READING
                    | CdRomStatus::SEEKING
                    | CdRomStatus::PLAYING
                    | CdRomStatus::ERROR,
            );

            reply_status(cdrom, int_ctrl, IrqFlag::Int3);
            cdrom.queue_task(Command::InitSecond, CDROM_SECOND_DELAY);
        }
        Command::InitSecond => {
            reply_status(cdrom, int_ctrl, IrqFlag::Int2);
        }
        Command::GetIdFirst => {
            reply_status(cdrom, int_ctrl, IrqFlag::Int3);
            cdrom.queue_task(Command::GetIdSecond, CDROM_SECOND_DELAY);
        }
        Command::GetIdSecond => {
            // Scotty doesn't know
            cdrom.push_response(&[0x02, 0x00, 0x20, 0x00, b'S', b'C', b'E', b'E']);
            cdrom.raise_int(int_ctrl, IrqFlag::Int2);
        }
        Command::Setfilter { file, channel } => {
            cdrom.filter_file = file;
            cdrom.filter_channel = channel;

            reply_status(cdrom, int_ctrl, IrqFlag::Int3);
        }
        Command::Setmode { mode } => {
            cdrom.mode = CdRomMode::from_bits_truncate(mode);

            reply_status(cdrom, int_ctrl, IrqFlag::Int3);
        }
        Command::Setloc { mm, ss, ff } => {
            let msf = Msf::from_bcd([mm, ss, ff]).ok_or(ErrorCode::BadParameter)?;
            cdrom.msf_loc = Some(msf);

            reply_status(cdrom, int_ctrl, IrqFlag::Int3);
        }
        Command::SeekFirst => {
            cdrom.cancel_read();
            cdrom.apply_setloc();
            cdrom.status.insert(CdRomStatus::SEEKING);
            cdrom
                .status
                .remove(CdRomStatus::READING | CdRomStatus::PLAYING);

            reply_status(cdrom, int_ctrl, IrqFlag::Int3);
            cdrom.queue_task(Command::SeekSecond, CDROM_SEEK_DELAY);
        }
        Command::SeekSecond => {
            cdrom.status.remove(CdRomStatus::SEEKING);

            reply_status(cdrom, int_ctrl, IrqFlag::Int2);
        }
        Command::StopFirst => {
            clear_transfer(cdrom);
            cdrom
                .status
                .remove(CdRomStatus::READING | CdRomStatus::SEEKING | CdRomStatus::PLAYING);

            reply_status(cdrom, int_ctrl, IrqFlag::Int3);
            cdrom.queue_task(Command::StopSecond, CDROM_SECOND_DELAY);
        }
        Command::StopSecond => {
            cdrom.status.remove(CdRomStatus::MOTOR_ON);

            reply_status(cdrom, int_ctrl, IrqFlag::Int2);
        }
        Command::PauseFirst => {
            clear_transfer(cdrom);
            cdrom
                .status
                .remove(CdRomStatus::READING | CdRomStatus::PLAYING);

            reply_status(cdrom, int_ctrl, IrqFlag::Int3);
            cdrom.queue_task(Command::PauseSecond, CDROM_SECOND_DELAY);
        }
        Command::PauseSecond => {
            reply_status(cdrom, int_ctrl, IrqFlag::Int2);
        }
        Command::Mute => {
            cdrom.mute = true;

            reply_status(cdrom, int_ctrl, IrqFlag::Int3);
        }
        Command::Demute => {
            cdrom.mute = false;

            reply_status(cdrom, int_ctrl, IrqFlag::Int3);
        }
        Command::GetTn => {
            let disc = cdrom.disc.as_ref().ok_or(ErrorCode::NoDisc)?;
            let last = bin_to_bcd(disc.tracks().len() as u8);

            cdrom.push_response(&[cdrom.status.bits(), 0x01, last]);
            cdrom.raise_int(int_ctrl, IrqFlag::Int3);
        }
        Command::GetTd { track } => {
            let response = get_td_response(cdrom, track)?;

            cdrom.push_response(&response);
            cdrom.raise_int(int_ctrl, IrqFlag::Int3);
        }
        Command::GetlocP => {
            let response = get_loc_p_response(cdrom)?;

            cdrom.push_response(&response);
            cdrom.raise_int(int_ctrl, IrqFlag::Int3);
        }
        Command::Read => {
            cdrom.apply_setloc();
            start_transfer(cdrom, int_ctrl, CdRomStatus::READING);
        }
        Command::Play { track } => {
            set_play_position(cdrom, track)?;
            start_transfer(
                cdrom,
                int_ctrl,
                CdRomStatus::MOTOR_ON | CdRomStatus::PLAYING,
            );
        }
        Command::SectorReady { retry } => {
            let step = if cdrom.status.contains(CdRomStatus::PLAYING) {
                play_sector(cdrom, int_ctrl)
            } else if cdrom.status.contains(CdRomStatus::READING) {
                read_sector(cdrom, int_ctrl, retry)
            } else {
                return Ok(());
            };

            let retry = match step {
                SectorStep::Advance => {
                    cdrom.position_lba = cdrom.cursor_lba;
                    cdrom.cursor_lba = cdrom.cursor_lba.wrapping_add(1);
                    false
                }
                SectorStep::Retry => true,
                SectorStep::Stopped => return Ok(()),
            };

            cdrom.queue_task(Command::SectorReady { retry }, cdrom.read_sector_delay());
        }
    }

    Ok(())
}

fn reply_status(cdrom: &mut CdRom, int_ctrl: &mut InterruptController, irq: IrqFlag) {
    cdrom.push_response(&[cdrom.status.bits()]);
    cdrom.raise_int(int_ctrl, irq);
}

fn get_td_response(cdrom: &CdRom, track: u8) -> Result<[u8; 3], ErrorCode> {
    let disc = cdrom.disc.as_ref().ok_or(ErrorCode::NoDisc)?;
    let lba = if track == 0 {
        disc.sector_count()
    } else {
        disc.tracks()
            .get(bcd_to_bin(track).saturating_sub(1) as usize)
            .ok_or(ErrorCode::BadParameter)?
            .start_lba
    };
    let [minutes, seconds, _] = Msf::from_lba(lba).to_bcd();

    Ok([cdrom.status.bits(), minutes, seconds])
}

fn get_loc_p_response(cdrom: &CdRom) -> Result<[u8; 8], ErrorCode> {
    let disc = cdrom.disc.as_ref().ok_or(ErrorCode::NoDisc)?;
    let (number, track) = disc
        .track_at(cdrom.position_lba)
        .ok_or(ErrorCode::BadParameter)?;

    let [rel_m, rel_s, rel_f] =
        Msf::from_frames(cdrom.position_lba.abs_diff(track.start_lba)).to_bcd();
    let [abs_m, abs_s, abs_f] = Msf::from_lba(cdrom.position_lba).to_bcd();
    let index = u8::from(cdrom.position_lba >= track.start_lba);

    Ok([
        bin_to_bcd(number),
        index,
        rel_m,
        rel_s,
        rel_f,
        abs_m,
        abs_s,
        abs_f,
    ])
}

fn start_transfer(cdrom: &mut CdRom, int_ctrl: &mut InterruptController, status: CdRomStatus) {
    clear_transfer(cdrom);
    cdrom
        .status
        .remove(CdRomStatus::READING | CdRomStatus::SEEKING | CdRomStatus::PLAYING);
    cdrom.status.insert(status);

    reply_status(cdrom, int_ctrl, IrqFlag::Int3);
    cdrom.queue_task(
        Command::SectorReady { retry: false },
        cdrom.read_sector_delay(),
    );
}

fn clear_transfer(cdrom: &mut CdRom) {
    cdrom.cancel_read();
    cdrom.pending_sector = None;
    cdrom.data_fifo.clear();
}

fn set_play_position(cdrom: &mut CdRom, track: u8) -> Result<(), ErrorCode> {
    let disc = cdrom.disc.as_ref().ok_or(ErrorCode::NoDisc)?;

    let lba = if track == 0 {
        cdrom.msf_loc.map(Msf::to_lba).unwrap_or(cdrom.cursor_lba)
    } else {
        // Out-of-range track numbers restart the current track.
        disc.tracks()
            .get(bcd_to_bin(track).saturating_sub(1) as usize)
            .or_else(|| disc.track_at(cdrom.position_lba).map(|(_, track)| track))
            .ok_or(ErrorCode::BadParameter)?
            .start_lba
    };

    let (number, _) = disc.track_at(lba).ok_or(ErrorCode::BadParameter)?;

    cdrom.cursor_lba = lba;
    cdrom.position_lba = lba;
    cdrom.play_track = number;
    cdrom.msf_loc = None;

    Ok(())
}

fn read_sector(cdrom: &mut CdRom, int_ctrl: &mut InterruptController, retry: bool) -> SectorStep {
    if cdrom.pending_sector.is_some() {
        if !retry {
            tracing::warn!(
                next_lba = cdrom.cursor_lba,
                "cdrom sector pending, giving CPU second chance"
            );
            return SectorStep::Retry;
        }

        tracing::warn!(
            next_lba = cdrom.cursor_lba,
            "cdrom sector overrun: dropping previous pending sector"
        );
        cdrom.pending_sector = None;
    }

    let result = cdrom
        .disc
        .as_mut()
        .ok_or(ErrorCode::NoDisc)
        .and_then(|disc| {
            disc.read_sector(cdrom.cursor_lba)
                .ok_or(ErrorCode::BadParameter)
        });

    let raw_sector = match result {
        Ok(sector) => sector,
        Err(err) => {
            cdrom.status.remove(CdRomStatus::READING);
            cdrom.raise_err(int_ctrl, err);
            return SectorStep::Stopped;
        }
    };

    if !is_xa_audio_sector(cdrom, &raw_sector) {
        cdrom.pending_sector = Some(raw_sector);
        reply_status(cdrom, int_ctrl, IrqFlag::Int1);
    }

    SectorStep::Advance
}

fn play_sector(cdrom: &mut CdRom, int_ctrl: &mut InterruptController) -> SectorStep {
    const RELATIVE_TIME_FLAG: u8 = 0x80;

    let Some(disc) = cdrom.disc.as_ref() else {
        cdrom.status.remove(CdRomStatus::PLAYING);
        cdrom.raise_err(int_ctrl, ErrorCode::NoDisc);
        return SectorStep::Stopped;
    };

    let position = disc.track_at(cdrom.cursor_lba);
    let at_end = position.is_none();
    if at_end
        || (cdrom.mode.contains(CdRomMode::AUTOPAUSE)
            && position.is_some_and(|(track, _)| track != cdrom.play_track))
    {
        cdrom.status.remove(CdRomStatus::PLAYING);
        if at_end {
            cdrom.status.remove(CdRomStatus::MOTOR_ON);
        }
        cdrom.cursor_lba = cdrom.position_lba;
        reply_status(cdrom, int_ctrl, IrqFlag::Int4);
        return SectorStep::Stopped;
    }

    let (number, &track) = position.unwrap();
    cdrom.play_track = number;

    // Position and reports advance even while CD-DA sample mixing is unimplemented.
    if track.mode != TrackMode::Audio || !cdrom.mode.contains(CdRomMode::REPORT_INTS) {
        return SectorStep::Advance;
    }

    // https://github.com/stenzek/duckstation/blob/983acec71ec706c3d436f0c70e88e58f6c88df0a/src/core/cdrom.cpp#L4101-L4122
    let absolute = Msf::from_lba(cdrom.cursor_lba);
    let [minutes, seconds, frames] = match absolute.frame {
        0 | 20 | 40 | 60 => absolute.to_bcd(),
        10 | 30 | 50 | 70 => {
            let relative = Msf::from_frames(cdrom.cursor_lba.abs_diff(track.start_lba));
            let [minutes, seconds, frames] = relative.to_bcd();
            [minutes, seconds | RELATIVE_TIME_FLAG, frames]
        }
        _ => return SectorStep::Advance,
    };

    cdrom.push_response(&[
        cdrom.status.bits(),
        bin_to_bcd(cdrom.play_track),
        u8::from(cdrom.cursor_lba >= track.start_lba),
        minutes,
        seconds,
        frames,
        0,
        0,
    ]);
    cdrom.raise_int(int_ctrl, IrqFlag::Int1);

    SectorStep::Advance
}

fn is_xa_audio_sector(cdrom: &CdRom, raw: &RawSector) -> bool {
    let sector_mode = raw[15];
    let submode = raw[18];

    let audio = submode & 0x04 != 0;
    let realtime = submode & 0x40 != 0;

    sector_mode == 2 && cdrom.mode.contains(CdRomMode::XA_ADPCM) && audio && realtime
}
