use std::{
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom},
    path::Path,
    sync::{Arc, Mutex},
};

use playastation::{
    formats::cue::CueSheet,
    interfaces::disc::{
        RawSector, TrackMode,
        cue::{CueDisc, DiscSource},
    },
};

#[derive(Clone)]
pub struct FileSource {
    file: Arc<Mutex<File>>,
    byte_len: u64,
}

impl DiscSource for FileSource {
    type Error = io::Error;

    fn byte_len(&self) -> u64 {
        self.byte_len
    }

    fn read_at(&mut self, offset: u64, buffer: &mut RawSector) -> io::Result<()> {
        let mut file = self.file.lock().unwrap();
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(buffer)
    }
}

pub fn open(path: &Path) -> io::Result<CueDisc<FileSource>> {
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cue"))
    {
        return CueDisc::from_bin(open_source(path)?, TrackMode::Mode2)
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

    CueDisc::from_cue(&cue, sources)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn open_source(path: &Path) -> io::Result<FileSource> {
    let file = File::open(path)
        .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", path.display())))?;
    let byte_len = file.metadata()?.len();
    Ok(FileSource {
        file: Arc::new(Mutex::new(file)),
        byte_len,
    })
}
