pub const RAW_SECTOR_SIZE: usize = 2352;

/// A complete raw CD sector, including the synchronization prefix.
pub type RawSector = [u8; RAW_SECTOR_SIZE];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackMode {
    Audio,
    Mode1,
    Mode2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Track {
    /// Disc LBA of INDEX 01. Track numbers are one-based indices in [`Disc::tracks`].
    pub start_lba: usize,
    /// Sectors preceding INDEX 01 that belong to this track's INDEX 00.
    pub pregap_sectors: usize,
    pub mode: TrackMode,
}

pub trait Disc {
    fn read_sector(&mut self, lba: usize) -> Option<RawSector>;

    fn sector_count(&self) -> usize;

    /// Single-track raw BIN implementations can use the default.
    fn tracks(&self) -> &[Track] {
        &[Track {
            start_lba: 0,
            pregap_sectors: 0,
            mode: TrackMode::Mode2,
        }]
    }

    fn track_at(&self, lba: usize) -> Option<(u8, &Track)> {
        if lba >= self.sector_count() {
            return None;
        }

        self.tracks()
            .iter()
            .enumerate()
            .rev()
            .find(|(_, track)| lba >= track.start_lba.saturating_sub(track.pregap_sectors))
            .map(|(index, track)| ((index + 1) as u8, track))
    }
}
