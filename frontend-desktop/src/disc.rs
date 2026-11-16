use std::{
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};

use playastation::{
    disc::{RawSector, TrackMode},
    formats::cue::{
        io::{DiscImage, DiscSource},
        parser::CueSheet,
    },
};

pub struct FileSource {
    file: File,
    byte_len: u64,
}

impl DiscSource for FileSource {
    type Error = io::Error;

    fn byte_len(&self) -> u64 {
        self.byte_len
    }

    fn read_at(&mut self, offset: u64, buffer: &mut RawSector) -> io::Result<()> {
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(buffer)
    }
}

pub fn open(path: &Path) -> io::Result<DiscImage<FileSource>> {
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cue"))
    {
        return DiscImage::from_bin(open_source(path)?, TrackMode::Mode2)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error));
    }

    let text = fs::read_to_string(path)?;
    let cue = CueSheet::parse(&text).map_err(io::Error::other)?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let sources = cue
        .files
        .iter()
        .map(|file| open_source(&base.join(file.name.replace('\\', "/"))))
        .collect::<io::Result<Vec<_>>>()?;

    DiscImage::from_cue(&cue, sources)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn open_source(path: &Path) -> io::Result<FileSource> {
    let file = File::open(path)
        .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", path.display())))?;
    let byte_len = file.metadata()?.len();
    Ok(FileSource { file, byte_len })
}
