use alloc::{format, string::String, vec, vec::Vec};

use snafu::Snafu;

use crate::interfaces::disc::TrackMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueSheet<'a> {
    pub files: Vec<CueFile<'a>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueFile<'a> {
    pub name: &'a str,
    pub tracks: Vec<CueTrack>,
}

/// Track indexes and gaps are measured in sectors; indexes are file-relative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueTrack {
    pub mode: TrackMode,
    pub index0: Option<usize>,
    pub index1: usize,
    pub pregap: usize,
    pub postgap: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Snafu)]
#[snafu(display("CUE line {line}, column {column}: {message}"))]
pub struct CueError {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl<'a> CueSheet<'a> {
    pub fn parse(text: &'a str) -> Result<Self, CueError> {
        parser::sheet(text).map_err(|error| CueError {
            line: error.location.line,
            column: error.location.column,
            message: format!("expected {}", error.expected),
        })?
    }
}

peg::parser! {
    grammar parser() for str {
        inject source(input, _start, _end) -> &'input str { input }

        rule space() = quiet!{ [' ' | '\t']+ }
        rule padding() = quiet!{ [c if c != '\n' && c.is_whitespace()]* }
        rule eol() = padding() ("\n" / ![_])
        rule trivia() = quiet!{ (padding() ("\n" / metadata() eol()))* padding() }
        rule line_end() = eol() trivia()

        rule metadata()
            = ("REM" / "TITLE" / "PERFORMER" / "SONGWRITER"
                / "CATALOG" / "ISRC" / "FLAGS" / "CDTEXTFILE")
              (space() / &eol()) [^ '\n']*

        rule number() -> u8
            = n:$(['0'..='9']+) {? n.parse().or(Err("number from 0 to 255")) }

        rule track_number() -> u8
            = n:number() {?
                (1..=99).contains(&n).then_some(n).ok_or("track number from 01 to 99")
            }

        rule time() -> usize
            = m:number() ":" s:number() ":" f:number() {?
                if m > 99 || s >= 60 || f >= 75 {
                    Err("valid MM:SS:FF timestamp")
                } else {
                    Ok((usize::from(m) * 60 + usize::from(s)) * 75 + usize::from(f))
                }
            }

        rule name() -> &'input str
            = "\"" name:$([^ '"' | '\r' | '\n']+) "\"" { name }
            / name:$([^ ' ' | '\t' | '"' | '\r' | '\n']+) { name }

        rule mode() -> TrackMode
            = "AUDIO" { TrackMode::Audio }
            / "MODE1/2352" { TrackMode::Mode1 }
            / "MODE2/2352" { TrackMode::Mode2 }

        rule pregap() -> usize
            = "PREGAP" space() time:time() line_end() { time }

        rule postgap() -> usize
            = "POSTGAP" space() time:time() line_end() { time }

        rule index0() -> usize
            = "INDEX" space() "0"+ space() time:time() line_end() { time }

        rule index_time(index0: Option<usize>) -> usize
            = time:time() {?
                index0.is_none_or(|start| start <= time)
                    .then_some(time).ok_or("INDEX 01 at or after INDEX 00")
            }

        rule index1(index0: Option<usize>) -> usize
            = "INDEX" space() "0"* "1" space() time:index_time(index0) line_end() { time }

        rule extra_index_number()
            = n:number() {?
                (2..=99).contains(&n).then_some(()).ok_or("index number from 02 to 99")
            }

        rule extra_index()
            = "INDEX" space() extra_index_number() space() time() line_end()

        rule track() -> ParsedTrack
            = offset:position!() "TRACK" space() number:track_number() space() mode:mode() line_end()
              pregap:pregap()? index0:index0()? index1:index1(index0)
              extra_index()* postgap:postgap()? {
                ParsedTrack {
                    number,
                    offset,
                    track: CueTrack {
                        mode,
                        index0,
                        index1,
                        pregap: pregap.unwrap_or(0),
                        postgap: postgap.unwrap_or(0),
                    },
                }
            }

        rule file() -> (&'input str, Vec<ParsedTrack>)
            = "FILE" space() name:name() space() "BINARY" line_end() tracks:track()+ {
                (name, tracks)
            }

        pub rule sheet() -> Result<CueSheet<'input>, CueError>
            = "\u{feff}"* trivia() files:file()+ {
                let invalid = files.iter().flat_map(|(_, tracks)| tracks)
                    .enumerate().find(|(i, track)| usize::from(track.number) != i + 1);
                if let Some((_, track)) = invalid {
                    let location = peg::Parse::position_repr(source, track.offset);
                    Err(CueError {
                        line: location.line,
                        column: location.column,
                        message: String::from("tracks must be numbered consecutively from 01 to 99"),
                    })
                } else {
                    Ok(CueSheet {
                        files: files.into_iter().map(|(name, tracks)| CueFile {
                            name,
                            tracks: tracks.into_iter().map(|track| track.track).collect(),
                        }).collect(),
                    })
                }
            }
    }
}

struct ParsedTrack {
    number: u8,
    offset: usize,
    track: CueTrack,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiple_files_and_stored_pregap() {
        let text = r#"REM test
FILE "Data (Track 1).bin" BINARY
  TRACK 01 MODE2/2352
    INDEX 01 00:00:00
FILE "Audio.bin" BINARY
  TRACK 02 AUDIO
    INDEX 00 00:00:00
    INDEX 01 00:02:00
"#;
        let text = format!("\u{feff}{}", text.replace('\n', "\r\n"));
        let cue = CueSheet::parse(&text).unwrap();

        assert_eq!(cue.files.len(), 2);
        assert_eq!(cue.files[0].name, "Data (Track 1).bin");
        assert_eq!(cue.files[0].tracks[0].mode, TrackMode::Mode2);
        assert_eq!(cue.files[1].name, "Audio.bin");
        assert_eq!(
            cue.files[1].tracks.as_slice(),
            &[CueTrack {
                mode: TrackMode::Audio,
                index0: Some(0),
                index1: 150,
                pregap: 0,
                postgap: 0,
            }]
        );
    }

    #[test]
    fn shared_bin_indexes_and_gaps() {
        let cue = CueSheet::parse(
            r#"FILE disc.bin BINARY
  TRACK 01 MODE1/2352
    INDEX 00 00:00:00
    INDEX 01 00:02:00
  TRACK 02 AUDIO
    PREGAP 00:02:00
    INDEX 00 00:10:00
    INDEX 01 00:12:00
    POSTGAP 00:01:00
"#,
        )
        .unwrap();

        assert_eq!(cue.files.len(), 1);
        assert_eq!(cue.files[0].name, "disc.bin");
        assert_eq!(
            cue.files[0].tracks.as_slice(),
            &[
                CueTrack {
                    mode: TrackMode::Mode1,
                    index0: Some(0),
                    index1: 150,
                    pregap: 0,
                    postgap: 0,
                },
                CueTrack {
                    mode: TrackMode::Audio,
                    index0: Some(750),
                    index1: 900,
                    pregap: 150,
                    postgap: 75,
                },
            ]
        );
    }

    #[test]
    fn metadata_and_whitespace() {
        let cue = CueSheet::parse(
            r#"REM
TITLE "Test disc"
PERFORMER "Artist"
SONGWRITER "Writer"
CATALOG 1234567890123
CDTEXTFILE "disc.cdt"
FILE	"音楽 disc.bin"	BINARY
  TRACK	01	AUDIO
    ISRC TEST
    FLAGS DCP
    INDEX	01	01:02:03
"#,
        )
        .unwrap();

        assert_eq!(cue.files.len(), 1);
        assert_eq!(cue.files[0].name, "音楽 disc.bin");
        assert_eq!(cue.files[0].tracks[0].index1, (60 + 2) * 75 + 3);
    }

    #[test]
    fn reports_syntax_error_line() {
        let error = CueSheet::parse(
            r#"FILE a.bin BINARY

  TRACK 01 AUDIO
    INDEX 01 00:60:00
"#,
        )
        .unwrap_err();

        assert_eq!((error.line, error.column), (4, 22));
        assert!(error.message.contains("MM:SS:FF"), "{error}");
    }

    #[test]
    fn requires_index1_in_every_track() {
        for input in [
            r#"FILE a.bin BINARY
  TRACK 01 AUDIO
"#,
            r#"FILE a.bin BINARY
  TRACK 01 AUDIO
  TRACK 02 AUDIO
    INDEX 01 00:00:00
"#,
            r#"FILE a.bin BINARY
  TRACK 01 AUDIO
FILE b.bin BINARY
  TRACK 02 AUDIO
    INDEX 01 00:00:00
"#,
        ] {
            let error = CueSheet::parse(input).unwrap_err();
            assert_eq!(error.line, 3, "{input}");
            assert!(error.message.contains("INDEX"), "{error}: {input}");
        }
    }

    #[test]
    fn requires_tracks_in_every_file() {
        for (input, line) in [
            (
                r#"FILE empty.bin BINARY
"#,
                2,
            ),
            (
                r#"FILE empty.bin BINARY
FILE next.bin BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00
"#,
                2,
            ),
            (
                r#"FILE a.bin BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00
FILE empty.bin BINARY
"#,
                5,
            ),
        ] {
            let error = CueSheet::parse(input).unwrap_err();
            assert_eq!(error.line, line, "{input}");
            assert!(error.message.contains("TRACK"), "{error}: {input}");
        }
    }

    #[test]
    fn metadata_between_fields_and_optional_final_newline() {
        let input = r#"
REM disc metadata
FILE disc.bin BINARY
  TITLE "File metadata"
  TRACK 01 AUDIO
    REM before pregap
    PREGAP 00:00:00
    TITLE "Track metadata"
    INDEX 00 00:00:00

    REM between indexes
    INDEX 01 00:01:00
    PERFORMER "Artist"
    INDEX 02 00:02:00
    INDEX 99 00:03:00
    REM before postgap
    POSTGAP 00:00:00"#;

        for suffix in ["", "\n", "\r\nREM trailing comment"] {
            let text = format!("{input}{suffix}");
            let cue = CueSheet::parse(&text).unwrap();
            assert_eq!(
                cue.files[0].tracks.as_slice(),
                &[CueTrack {
                    mode: TrackMode::Audio,
                    index0: Some(0),
                    index1: 75,
                    pregap: 0,
                    postgap: 0,
                }]
            );
        }
    }

    #[test]
    fn track_numbers_are_consecutive_across_files() {
        for input in [
            r#"FILE a.bin BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00
FILE b.bin BINARY
  TRACK 03 AUDIO
    INDEX 01 00:00:00
"#,
            r#"FILE a.bin BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00
FILE b.bin BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00
"#,
        ] {
            let error = CueSheet::parse(input).unwrap_err();
            assert_eq!((error.line, error.column), (5, 3), "{input}");
            assert!(error.message.contains("consecutively"), "{error}: {input}");
        }
    }

    #[test]
    fn rejects_repeated_or_misplaced_track_fields() {
        for (input, line) in [
            (
                r#"FILE a.bin BINARY
  TRACK 01 AUDIO
    PREGAP 00:00:00
    PREGAP 00:02:00
    INDEX 01 00:00:00
"#,
                4,
            ),
            (
                r#"FILE a.bin BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00
    POSTGAP 00:00:00
    POSTGAP 00:01:00
"#,
                5,
            ),
            (
                r#"FILE a.bin BINARY
  TRACK 01 AUDIO
    INDEX 00 00:00:00
    PREGAP 00:02:00
    INDEX 01 00:02:00
"#,
                4,
            ),
            (
                r#"FILE a.bin BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00
    POSTGAP 00:01:00
    INDEX 02 00:02:00
"#,
                5,
            ),
            (
                r#"FILE a.bin BINARY
  TRACK 01 AUDIO
    INDEX 00 00:00:00
    INDEX 00 00:00:00
    INDEX 01 00:02:00
"#,
                4,
            ),
            (
                r#"FILE a.bin BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00
    INDEX 01 00:02:00
"#,
                4,
            ),
            (
                r#"FILE a.bin BINARY
  TRACK 01 AUDIO
    INDEX 00 00:02:00
    INDEX 01 00:01:00
"#,
                4,
            ),
            (
                r#"FILE a.bin BINARY
  TRACK 01 AUDIO
    POSTGAP 00:00:00
    INDEX 01 00:00:00
"#,
                3,
            ),
        ] {
            let error = CueSheet::parse(input).unwrap_err();
            assert_eq!(error.line, line, "{error}: {input}");
        }
    }

    #[test]
    fn rejects_invalid_syntax() {
        for input in [
            "",
            "TRACK 01 AUDIO",
            r#"FILE a.bin BINARY
  TRACK 02 AUDIO
    INDEX 01 00:00:00
"#,
            r#"FILE a.bin BINARY
  TRACK 01 AUDIO
"#,
            r#"FILE a.bin BINARY
  TRACK 01 AUDIO
    INDEX 01 00:60:00
"#,
            r#"FILE a.bin BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:75
"#,
            r#"FILE a.bin BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00 junk
"#,
            r#"FILE a.wav WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
"#,
            r#"FILE a.bin BINARY
  TRACK 01 AUDIO
    INDEX 01 100:00:00
"#,
            r#"FILE a.bin BINARY
  TRACK 01 AUDIO
    INDEX 256 00:00:00
"#,
            r#"FILE "" BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00
"#,
            r#"FILE "a.bin BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00
"#,
            r#"FILE a.bin BINARY
  TRACK01 AUDIO
    INDEX 01 00:00:00
"#,
            r#"REMINDER
FILE a.bin BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00
"#,
            r#"FILE a.bin BINARY
  TRACK 100 AUDIO
    INDEX 01 00:00:00
"#,
            r#"FILE a.bin BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00
    INDEX 100 00:02:00
"#,
            r#"FILE a.bin BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00
    INDEX 255 00:02:00
"#,
            r#"FILE a.bin BINARY TRACK 01 AUDIO
    INDEX 01 00:00:00
"#,
            r#"FILE a.bin BINARY
  TRACK 01
  AUDIO
    INDEX 01 00:00:00
"#,
            r#"FILE a.bin BINARY
  TRACK 01 AUDIO
    INDEX 01 00:00:00 POSTGAP 00:00:00
"#,
        ] {
            assert!(CueSheet::parse(input).is_err(), "{input}");
        }
    }
}
