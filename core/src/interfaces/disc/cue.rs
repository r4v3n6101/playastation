use alloc::{string::String, vec, vec::Vec};
use core::ops::Range;

use snafu::{Snafu, ensure};

use crate::formats::cue::{CueFile, CueSheet, CueTrack};

use super::{Disc, RAW_SECTOR_SIZE, RawSector, Track, TrackMode};

/// A byte source. Its length must remain unchanged while the image is mounted.
pub trait DiscSource {
    type Error;

    fn byte_len(&self) -> u64;

    fn read_at(&mut self, offset: u64, buffer: &mut RawSector) -> Result<(), Self::Error>;
}

#[derive(Debug, Snafu)]
#[snafu(context(suffix(false)))]
pub enum CueDiscError {
    #[snafu(display("expected {expected} BIN sources, got {actual}"))]
    SourceCount { expected: usize, actual: usize },
    #[snafu(display(
        "{} must have a nonzero length divisible by {RAW_SECTOR_SIZE}",
        file_name.as_deref().unwrap_or("BIN source")
    ))]
    InvalidSourceSize { file_name: Option<String> },

    #[snafu(display("disc and every BIN source must contain tracks"))]
    NoTracks,
    #[snafu(display("track {track} indexes overlap or exceed BIN length"))]
    InvalidTrack { track: usize },

    #[snafu(display("disc length overflow"))]
    TooLarge,
}

pub struct CueDisc<S> {
    tracks: Vec<Track>,
    track_data: Vec<TrackData<S>>,
    sector_count: usize,
}

struct TrackData<S> {
    source: S,
    disc_range: Range<usize>,
    file_sector: usize,
}

impl<S: DiscSource> CueDisc<S> {
    /// Sources correspond to FILE entries, in the same order.
    /// Each source is cloned for its tracks.
    pub fn from_cue(cue: &CueSheet<'_>, sources: Vec<S>) -> Result<Self, CueDiscError>
    where
        S: Clone,
    {
        ensure!(
            sources.len() == cue.files.len(),
            SourceCount {
                expected: cue.files.len(),
                actual: sources.len(),
            }
        );
        ensure!(
            !cue.files.is_empty() && cue.files.iter().all(|file| !file.tracks.is_empty()),
            NoTracks
        );

        let mut image = Self {
            tracks: Vec::new(),
            track_data: Vec::new(),
            sector_count: 0,
        };

        for (file, source) in cue.files.iter().zip(sources) {
            let file_sectors = sectors_in_source(source.byte_len(), Some(file.name))?;
            image.append_file(source, file, file_sectors)?;
        }

        Ok(image)
    }

    pub fn from_bin(source: S, mode: TrackMode) -> Result<Self, CueDiscError> {
        let sector_count = sectors_in_source(source.byte_len(), None)?;
        Ok(Self {
            tracks: vec![Track {
                start_lba: 0,
                pregap_sectors: 0,
                mode,
            }],
            track_data: vec![TrackData {
                source,
                disc_range: 0..sector_count,
                file_sector: 0,
            }],
            sector_count,
        })
    }

    fn append_file(
        &mut self,
        source: S,
        file: &CueFile<'_>,
        file_sectors: usize,
    ) -> Result<(), CueDiscError>
    where
        S: Clone,
    {
        let mut tracks = file.tracks.iter().peekable();

        while let Some(track) = tracks.next() {
            // The first track's lead-in/pregap is outside disc LBA 0.
            let file_start = if self.tracks.is_empty() {
                track.index1
            } else {
                track.index0.unwrap_or(track.index1)
            };
            let file_end = tracks
                .peek()
                .map_or(file_sectors, |next| next.index0.unwrap_or(next.index1));

            let file_range = file_start..file_end;
            ensure!(
                file_range.contains(&track.index1) && file_end <= file_sectors,
                InvalidTrack {
                    track: self.tracks.len() + 1,
                }
            );

            self.append_track(source.clone(), track, file_range)?;
        }
        Ok(())
    }

    fn append_track(
        &mut self,
        source: S,
        track: &CueTrack,
        file_range: Range<usize>,
    ) -> Result<(), CueDiscError> {
        let pregap = if self.tracks.is_empty() {
            0
        } else {
            track.pregap
        };
        let pregap_start = self.sector_count;
        let data_start = pregap_start
            .checked_add(pregap)
            .ok_or(CueDiscError::TooLarge)?;
        let data_end = data_start
            .checked_add(file_range.len())
            .ok_or(CueDiscError::TooLarge)?;
        let end = data_end
            .checked_add(track.postgap)
            .ok_or(CueDiscError::TooLarge)?;
        let start_lba = data_start + (track.index1 - file_range.start);

        self.tracks.push(Track {
            start_lba,
            pregap_sectors: start_lba - pregap_start,
            mode: track.mode,
        });
        self.track_data.push(TrackData {
            source,
            disc_range: data_start..data_end,
            file_sector: file_range.start,
        });
        self.sector_count = end;

        Ok(())
    }
}

impl<S: DiscSource> Disc for CueDisc<S> {
    fn read_sector(&mut self, lba: usize) -> Option<RawSector> {
        if lba >= self.sector_count {
            return None;
        }

        let data = self
            .track_data
            .iter_mut()
            .find(|data| data.disc_range.contains(&lba));

        // Sectors outside file data but within the disc are virtual gaps.
        let mut sector = [0; RAW_SECTOR_SIZE];
        if let Some(data) = data {
            let file_sector = data.file_sector + (lba - data.disc_range.start);
            let offset = file_sector as u64 * RAW_SECTOR_SIZE as u64;
            data.source.read_at(offset, &mut sector).ok()?;
        }

        Some(sector)
    }

    fn sector_count(&self) -> usize {
        self.sector_count
    }

    fn tracks(&self) -> &[Track] {
        &self.tracks
    }
}

fn sectors_in_source(byte_len: u64, file_name: Option<&str>) -> Result<usize, CueDiscError> {
    ensure!(
        byte_len != 0 && byte_len.is_multiple_of(RAW_SECTOR_SIZE as u64),
        InvalidSourceSize {
            file_name: file_name.map(String::from),
        }
    );

    usize::try_from(byte_len / RAW_SECTOR_SIZE as u64).map_err(|_| CueDiscError::TooLarge)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct MemorySource(Vec<u8>);

    impl DiscSource for MemorySource {
        type Error = ();

        fn byte_len(&self) -> u64 {
            self.0.len() as u64
        }

        fn read_at(&mut self, offset: u64, buffer: &mut RawSector) -> Result<(), Self::Error> {
            let start = usize::try_from(offset).map_err(|_| ())?;
            let end = start.checked_add(buffer.len()).ok_or(())?;
            buffer.copy_from_slice(self.0.get(start..end).ok_or(())?);
            Ok(())
        }
    }

    fn source(sectors: &[u8]) -> MemorySource {
        MemorySource(
            sectors
                .iter()
                .flat_map(|&value| core::iter::repeat_n(value, RAW_SECTOR_SIZE))
                .collect(),
        )
    }

    #[test]
    fn single_bin_reads_whole_sectors_in_any_order() {
        let mut disc = CueDisc::from_bin(source(&[0x11, 0x22, 0x33]), TrackMode::Mode2).unwrap();
        assert_eq!(disc.sector_count(), 3);
        assert_eq!(
            disc.tracks(),
            &[Track {
                start_lba: 0,
                pregap_sectors: 0,
                mode: TrackMode::Mode2,
            }]
        );
        for (lba, value) in [(2, 0x33), (0, 0x11), (1, 0x22)] {
            assert_eq!(disc.read_sector(lba), Some([value; RAW_SECTOR_SIZE]));
        }
        assert_eq!(disc.read_sector(3), None);
        assert_eq!(disc.read_sector(usize::MAX), None);
        assert!(disc.track_at(3).is_none());
    }

    #[test]
    fn shared_bin_maps_stored_and_virtual_gaps() {
        let cue = CueSheet::parse(
            r#"FILE shared.bin BINARY
  TRACK 01 MODE2/2352
    PREGAP 00:00:07
    INDEX 00 00:00:00
    INDEX 01 00:00:02
  TRACK 02 AUDIO
    PREGAP 00:00:02
    INDEX 00 00:00:04
    INDEX 01 00:00:05
    POSTGAP 00:00:01
"#,
        )
        .unwrap();
        let mut disc = CueDisc::from_cue(
            &cue,
            vec![source(&[0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17])],
        )
        .unwrap();

        assert_eq!(disc.sector_count(), 9);
        assert_eq!(
            disc.tracks(),
            &[
                Track {
                    start_lba: 0,
                    pregap_sectors: 0,
                    mode: TrackMode::Mode2,
                },
                Track {
                    start_lba: 5,
                    pregap_sectors: 3,
                    mode: TrackMode::Audio,
                },
            ]
        );
        for (lba, value) in [0x12, 0x13, 0, 0, 0x14, 0x15, 0x16, 0x17, 0]
            .into_iter()
            .enumerate()
        {
            assert_eq!(disc.read_sector(lba), Some([value; RAW_SECTOR_SIZE]));
            assert_eq!(disc.track_at(lba).unwrap().0, if lba < 2 { 1 } else { 2 });
        }
        assert_eq!(disc.read_sector(9), None);
    }

    #[test]
    fn separate_bins_use_file_relative_offsets() {
        let cue = CueSheet::parse(
            r#"FILE data.bin BINARY
  TRACK 01 MODE1/2352
    INDEX 01 00:00:00
FILE audio.bin BINARY
  TRACK 02 AUDIO
    INDEX 00 00:00:01
    INDEX 01 00:00:02
"#,
        )
        .unwrap();
        let mut disc = CueDisc::from_cue(
            &cue,
            vec![source(&[0xa1, 0xa2]), source(&[0xb0, 0xb1, 0xb2, 0xb3])],
        )
        .unwrap();

        assert_eq!(disc.sector_count(), 5);
        assert_eq!(disc.tracks()[1].start_lba, 3);
        assert_eq!(disc.tracks()[1].pregap_sectors, 1);
        for (lba, value) in [(4, 0xb3), (1, 0xa2), (2, 0xb1), (0, 0xa1), (3, 0xb2)] {
            assert_eq!(disc.read_sector(lba), Some([value; RAW_SECTOR_SIZE]));
        }
        assert_eq!(disc.track_at(2).unwrap().0, 2);
        assert_eq!(disc.read_sector(5), None);
    }

    #[test]
    fn rejects_empty_or_partial_sources() {
        for size in [0, RAW_SECTOR_SIZE - 1, RAW_SECTOR_SIZE + 1] {
            assert!(matches!(
                CueDisc::from_bin(MemorySource(vec![0; size]), TrackMode::Mode2),
                Err(CueDiscError::InvalidSourceSize { file_name: None })
            ));
        }

        let cue = CueSheet::parse(
            r#"FILE data.bin BINARY
  TRACK 01 MODE2/2352
    INDEX 01 00:00:00
FILE "audio/Track 02.bin" BINARY
  TRACK 02 AUDIO
    INDEX 01 00:00:00
"#,
        )
        .unwrap();
        let error = CueDisc::from_cue(&cue, vec![source(&[1]), source(&[])])
            .err()
            .unwrap();
        assert!(
            matches!(&error, CueDiscError::InvalidSourceSize { file_name: Some(name) }
            if name == "audio/Track 02.bin")
        );
        assert!(alloc::format!("{error}").contains("audio/Track 02.bin"));
    }

    #[test]
    fn rejects_overlapping_or_out_of_bounds_tracks() {
        for text in [
            r#"FILE disc.bin BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:03
"#,
            r#"FILE disc.bin BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:02
  TRACK 02 AUDIO
    INDEX 00 00:00:01
    INDEX 01 00:00:02
"#,
            r#"FILE disc.bin BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    INDEX 01 00:00:04
"#,
        ] {
            let cue = CueSheet::parse(text).unwrap();
            assert!(matches!(
                CueDisc::from_cue(&cue, vec![source(&[1, 2, 3])]),
                Err(CueDiscError::InvalidTrack { track: 1 })
            ));
        }
    }

    #[test]
    fn requires_sources_and_tracks() {
        let cue = CueSheet::parse(
            r#"FILE disc.bin BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00
"#,
        )
        .unwrap();
        for sources in [vec![], vec![source(&[1]), source(&[2])]] {
            let count = sources.len();
            assert!(matches!(
                CueDisc::from_cue(&cue, sources),
                Err(CueDiscError::SourceCount { expected: 1, actual }) if actual == count
            ));
        }
        assert!(matches!(
            CueDisc::<MemorySource>::from_cue(&CueSheet { files: vec![] }, vec![]),
            Err(CueDiscError::NoTracks)
        ));
        assert!(matches!(
            CueDisc::from_cue(
                &CueSheet {
                    files: vec![CueFile {
                        name: "empty.bin",
                        tracks: vec![]
                    }],
                },
                vec![source(&[1])],
            ),
            Err(CueDiscError::NoTracks)
        ));
    }

    #[test]
    fn rejects_disc_length_overflow() {
        let mut cue = CueSheet::parse(
            r#"FILE disc.bin BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00
"#,
        )
        .unwrap();
        cue.files[0].tracks[0].postgap = usize::MAX;
        assert!(matches!(
            CueDisc::from_cue(&cue, vec![source(&[1])]),
            Err(CueDiscError::TooLarge)
        ));
    }
}
