use alloc::{vec, vec::Vec};
use core::ops::Range;

use snafu::{OptionExt as _, Snafu, ensure};

use crate::disc::{Disc, RAW_SECTOR_SIZE, RawSector, Track, TrackMode};

use super::parser::{CueFile, CueSheet, CueTrack};

/// A byte source. Its length must remain unchanged while the image is mounted.
pub trait DiscSource {
    type Error;

    fn byte_len(&self) -> u64;

    fn read_at(&mut self, offset: u64, buffer: &mut RawSector) -> Result<(), Self::Error>;
}

pub struct DiscImage<S> {
    sources: Vec<S>,
    tracks: Vec<Track>,
    segments: Vec<Segment>,
    sector_count: usize,
}

struct Segment {
    disc_range: Range<usize>,
    source: SegmentSource,
}

#[derive(Clone, Copy)]
enum SegmentSource {
    Data { source: usize, file_sector: usize },
    Zero,
}

#[derive(Debug, Snafu)]
pub enum DiscImageError {
    #[snafu(display("expected {expected} BIN sources, got {actual}"))]
    SourceCount { expected: usize, actual: usize },

    #[snafu(display("disc and every BIN source must contain tracks"))]
    NoTracks,

    #[snafu(display(
        "BIN source {} must have a nonzero length divisible by {RAW_SECTOR_SIZE}",
        source + 1
    ))]
    InvalidSourceSize {
        #[snafu(source(false))]
        source: usize,
    },

    #[snafu(display("track {track} indexes overlap or exceed BIN length"))]
    InvalidTrack { track: usize },

    #[snafu(display("disc length overflow"))]
    TooLarge,
}

impl<S: DiscSource> DiscImage<S> {
    /// Sources correspond to FILE entries, in the same order.
    pub fn from_cue(cue: &CueSheet<'_>, sources: Vec<S>) -> Result<Self, DiscImageError> {
        ensure!(
            sources.len() == cue.files.len(),
            SourceCountSnafu {
                expected: cue.files.len(),
                actual: sources.len(),
            }
        );
        ensure!(
            !cue.files.is_empty() && cue.files.iter().all(|file| !file.tracks.is_empty()),
            NoTracksSnafu
        );

        let mut image = Self {
            sources,
            tracks: Vec::new(),
            segments: Vec::new(),
            sector_count: 0,
        };

        for (source, file) in cue.files.iter().enumerate() {
            image.append_file(source, file)?;
        }

        Ok(image)
    }

    pub fn from_bin(source: S, mode: TrackMode) -> Result<Self, DiscImageError> {
        let sector_count = sectors_in_source(source.byte_len(), 0)?;
        Ok(Self {
            sources: vec![source],
            tracks: vec![Track {
                start_lba: 0,
                pregap_sectors: 0,
                mode,
            }],
            segments: vec![Segment {
                disc_range: 0..sector_count,
                source: SegmentSource::Data {
                    source: 0,
                    file_sector: 0,
                },
            }],
            sector_count,
        })
    }

    fn append_file(&mut self, source: usize, file: &CueFile<'_>) -> Result<(), DiscImageError> {
        let file_sectors = sectors_in_source(self.sources[source].byte_len(), source)?;
        for (index, track) in file.tracks.iter().enumerate() {
            // The first track's lead-in/pregap is outside disc LBA 0.
            let file_start = if self.tracks.is_empty() {
                track.index1
            } else {
                track.index0.unwrap_or(track.index1)
            };
            let file_end = file
                .tracks
                .get(index + 1)
                .map_or(file_sectors, |next| next.index0.unwrap_or(next.index1));
            ensure!(
                file_start <= track.index1 && track.index1 < file_end && file_end <= file_sectors,
                InvalidTrackSnafu {
                    track: self.tracks.len() + 1,
                }
            );

            self.append_track(source, track, file_start..file_end)?;
        }
        Ok(())
    }

    fn append_track(
        &mut self,
        source: usize,
        track: &CueTrack,
        file_range: Range<usize>,
    ) -> Result<(), DiscImageError> {
        let pregap = if self.tracks.is_empty() {
            0
        } else {
            track.pregap
        };
        let pregap_start = self.append(pregap, SegmentSource::Zero)?;
        let data_start = self.append(
            file_range.end - file_range.start,
            SegmentSource::Data {
                source,
                file_sector: file_range.start,
            },
        )?;
        let start_lba = data_start + (track.index1 - file_range.start);
        self.tracks.push(Track {
            start_lba,
            pregap_sectors: start_lba - pregap_start,
            mode: track.mode,
        });
        self.append(track.postgap, SegmentSource::Zero)?;
        Ok(())
    }

    fn append(&mut self, sectors: usize, source: SegmentSource) -> Result<usize, DiscImageError> {
        let start = self.sector_count;
        let end = start.checked_add(sectors).context(TooLargeSnafu)?;
        if sectors != 0 {
            self.segments.push(Segment {
                disc_range: start..end,
                source,
            });
        }
        self.sector_count = end;
        Ok(start)
    }
}

impl<S: DiscSource> Disc for DiscImage<S> {
    fn read_sector(&mut self, lba: usize) -> Option<RawSector> {
        let segment = self
            .segments
            .iter()
            .find(|segment| segment.disc_range.contains(&lba))?;
        let mut sector = [0; RAW_SECTOR_SIZE];
        if let SegmentSource::Data {
            source,
            file_sector,
        } = segment.source
        {
            let file_sector = file_sector + (lba - segment.disc_range.start);
            let offset = file_sector as u64 * RAW_SECTOR_SIZE as u64;
            self.sources[source].read_at(offset, &mut sector).ok()?;
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

fn sectors_in_source(byte_len: u64, source: usize) -> Result<usize, DiscImageError> {
    ensure!(
        byte_len != 0 && byte_len.is_multiple_of(RAW_SECTOR_SIZE as u64),
        InvalidSourceSizeSnafu { source }
    );

    usize::try_from(byte_len / RAW_SECTOR_SIZE as u64).map_err(|_| DiscImageError::TooLarge)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut disc = DiscImage::from_bin(source(&[0x11, 0x22, 0x33]), TrackMode::Mode2).unwrap();
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
        let mut disc = DiscImage::from_cue(
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
        let mut disc = DiscImage::from_cue(
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
                DiscImage::from_bin(MemorySource(vec![0; size]), TrackMode::Mode2),
                Err(DiscImageError::InvalidSourceSize { source: 0 })
            ));
        }

        let cue = CueSheet::parse(
            r#"FILE data.bin BINARY
  TRACK 01 MODE2/2352
    INDEX 01 00:00:00
FILE empty.bin BINARY
  TRACK 02 AUDIO
    INDEX 01 00:00:00
"#,
        )
        .unwrap();
        assert!(matches!(
            DiscImage::from_cue(&cue, vec![source(&[1]), source(&[])]),
            Err(DiscImageError::InvalidSourceSize { source: 1 })
        ));
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
                DiscImage::from_cue(&cue, vec![source(&[1, 2, 3])]),
                Err(DiscImageError::InvalidTrack { track: 1 })
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
                DiscImage::from_cue(&cue, sources),
                Err(DiscImageError::SourceCount { expected: 1, actual }) if actual == count
            ));
        }
        assert!(matches!(
            DiscImage::<MemorySource>::from_cue(&CueSheet { files: vec![] }, vec![]),
            Err(DiscImageError::NoTracks)
        ));
        assert!(matches!(
            DiscImage::from_cue(
                &CueSheet {
                    files: vec![CueFile {
                        name: "empty.bin",
                        tracks: vec![]
                    }],
                },
                vec![source(&[1])],
            ),
            Err(DiscImageError::NoTracks)
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
            DiscImage::from_cue(&cue, vec![source(&[1])]),
            Err(DiscImageError::TooLarge)
        ));
    }
}
