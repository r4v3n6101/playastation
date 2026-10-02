use crate::{SystemTime, devices::int::InterruptController};

use super::{
    CDROM_SECOND_DELAY, CDROM_SEEK_DELAY, CdRom, CdRomMode, CdRomStatus, ErrorCode, IrqFlag,
    bin_to_bcd,
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
    SectorReady,
    PauseFirst,
    PauseSecond,
    Mute,
    Demute,
    GetTn,
    GetTd { track: u8 },
}

pub struct ScheduledCommand {
    pub remaining_delay: SystemTime,
    pub cmd: Command,
}

impl Command {
    pub fn busy_flag(self) -> bool {
        !matches!(
            self,
            Self::InitSecond
                | Self::GetIdSecond
                | Self::SeekSecond
                | Self::SectorReady
                | Self::PauseSecond
        )
    }

    pub fn execute(self, cdrom: &mut CdRom, int_ctrl: &mut InterruptController) {
        match self {
            Self::Bad { cmd } => {
                tracing::warn!(cmd=%format_args!("{:#X}", cmd), "bad cdrom command");

                cdrom.raise_err(ErrorCode::BadCommand, int_ctrl);
            }
            Self::Test { subcommand: _ } => {
                cdrom.push_response(&[0x94, 0x09, 0x19, 0xC0]);
                cdrom.raise_int(IrqFlag::Int3, int_ctrl);
            }
            Self::Getstat => {
                cdrom.push_response(&[cdrom.status.bits()]);
                cdrom.raise_int(IrqFlag::Int3, int_ctrl);
            }
            Self::InitFirst => {
                cdrom.cancel_read();

                cdrom.mode = CdRomMode::empty();

                cdrom.status.remove(
                    CdRomStatus::READING
                        | CdRomStatus::SEEKING
                        | CdRomStatus::PLAYING
                        | CdRomStatus::ERROR,
                );

                cdrom.data_fifo.clear();
                cdrom.pending_sector = None;

                cdrom.push_response(&[cdrom.status.bits()]);
                cdrom.raise_int(IrqFlag::Int3, int_ctrl);

                cdrom.queue_task(Self::InitSecond, CDROM_SECOND_DELAY);
            }
            Self::InitSecond => {
                cdrom.push_response(&[cdrom.status.bits()]);
                cdrom.raise_int(IrqFlag::Int2, int_ctrl);
            }
            Self::GetIdFirst => {
                cdrom.push_response(&[cdrom.status.bits()]);
                cdrom.raise_int(IrqFlag::Int3, int_ctrl);

                cdrom.queue_task(Self::GetIdSecond, CDROM_SECOND_DELAY);
            }
            Self::GetIdSecond => {
                // Eurotour!
                cdrom.push_response(&[0x02, 0x00, 0x20, 0x00, b'S', b'C', b'E', b'E']);
                cdrom.raise_int(IrqFlag::Int2, int_ctrl);
            }
            Self::Setfilter { file, channel } => {
                cdrom.filter_file = file;
                cdrom.filter_channel = channel;

                cdrom.push_response(&[cdrom.status.bits()]);
                cdrom.raise_int(IrqFlag::Int3, int_ctrl);
            }
            Self::Setmode { mode } => {
                cdrom.mode = CdRomMode::from_bits_truncate(mode);

                cdrom.push_response(&[cdrom.status.bits()]);
                cdrom.raise_int(IrqFlag::Int3, int_ctrl);
            }
            Self::Setloc { mm, ss, ff } => {
                cdrom.msf_loc = Some([mm, ss, ff]);

                cdrom.push_response(&[cdrom.status.bits()]);
                cdrom.raise_int(IrqFlag::Int3, int_ctrl);
            }
            Self::SeekFirst => {
                cdrom.cancel_read();

                cdrom.apply_setloc();

                cdrom.status.insert(CdRomStatus::SEEKING);
                cdrom
                    .status
                    .remove(CdRomStatus::READING | CdRomStatus::PLAYING);

                cdrom.push_response(&[cdrom.status.bits()]);
                cdrom.raise_int(IrqFlag::Int3, int_ctrl);

                cdrom.queue_task(Self::SeekSecond, CDROM_SEEK_DELAY);
            }
            Self::SeekSecond => {
                cdrom.status.remove(CdRomStatus::SEEKING);

                cdrom.push_response(&[cdrom.status.bits()]);
                cdrom.raise_int(IrqFlag::Int2, int_ctrl);
            }
            Self::Read => {
                cdrom.cancel_read();

                cdrom.apply_setloc();

                cdrom.status.insert(CdRomStatus::READING);
                cdrom
                    .status
                    .remove(CdRomStatus::SEEKING | CdRomStatus::PLAYING);

                cdrom.pending_sector = None;
                cdrom.data_fifo.clear();
                cdrom.read_second_delivery_attempt = false;

                cdrom.push_response(&[cdrom.status.bits()]);
                cdrom.raise_int(IrqFlag::Int3, int_ctrl);

                cdrom.queue_task(Self::SectorReady, cdrom.read_sector_delay());
            }
            Self::SectorReady => {
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

                    cdrom.queue_task(Self::SectorReady, cdrom.read_sector_delay());
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
                    cdrom.raise_err(ErrorCode::NoDisc, int_ctrl);
                    return;
                };

                let Some(raw_sector) = disc.read_sector(cdrom.cursor_lba) else {
                    cdrom.status.remove(CdRomStatus::READING);
                    cdrom.read_second_delivery_attempt = false;
                    cdrom.raise_err(ErrorCode::BadParameter, int_ctrl);
                    return;
                };

                cdrom.cursor_lba = cdrom.cursor_lba.wrapping_add(1);

                if is_xa_audio_sector(cdrom, &raw_sector) {
                    cdrom.queue_task(Self::SectorReady, cdrom.read_sector_delay());

                    return;
                }

                cdrom.pending_sector = Some(raw_sector);
                cdrom.read_second_delivery_attempt = false;

                cdrom.push_response(&[cdrom.status.bits()]);
                cdrom.raise_int(IrqFlag::Int1, int_ctrl);

                cdrom.queue_task(Self::SectorReady, cdrom.read_sector_delay());
            }
            Self::PauseFirst => {
                cdrom.cancel_read();

                cdrom
                    .status
                    .remove(CdRomStatus::READING | CdRomStatus::PLAYING);

                cdrom.pending_sector = None;
                cdrom.data_fifo.clear();

                cdrom.push_response(&[cdrom.status.bits()]);
                cdrom.raise_int(IrqFlag::Int3, int_ctrl);

                cdrom.queue_task(Self::PauseSecond, CDROM_SECOND_DELAY);
            }
            Self::PauseSecond => {
                cdrom.push_response(&[cdrom.status.bits()]);
                cdrom.raise_int(IrqFlag::Int2, int_ctrl);
            }
            Self::Mute => {
                cdrom.mute = true;

                cdrom.push_response(&[cdrom.status.bits()]);
                cdrom.raise_int(IrqFlag::Int3, int_ctrl);
            }
            Self::Demute => {
                cdrom.mute = false;

                cdrom.push_response(&[cdrom.status.bits()]);
                cdrom.raise_int(IrqFlag::Int3, int_ctrl);
            }
            Self::GetTn => {
                // Single data track fallback.
                cdrom.push_response(&[
                    cdrom.status.bits(),
                    0x01, // first track, BCD
                    0x01, // last track, BCD
                ]);

                cdrom.raise_int(IrqFlag::Int3, int_ctrl);
            }
            Self::GetTd { track } => {
                let track = ((track >> 4) * 10) + (track & 0x0F);

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
                        cdrom.raise_err(ErrorCode::BadParameter, int_ctrl);
                        return;
                    }
                };

                cdrom.push_response(&[
                    cdrom.status.bits(),
                    bin_to_bcd(minutes),
                    bin_to_bcd(seconds),
                ]);

                cdrom.raise_int(IrqFlag::Int3, int_ctrl);
            }
        }
    }
}

fn is_xa_audio_sector(cdrom: &CdRom, raw: &[u8]) -> bool {
    let sector_mode = raw[3];
    let submode = raw[6];

    let audio = submode & 0x04 != 0;
    let realtime = submode & 0x40 != 0;

    sector_mode == 2 && cdrom.mode.contains(CdRomMode::XA_ADPCM) && audio && realtime
}
