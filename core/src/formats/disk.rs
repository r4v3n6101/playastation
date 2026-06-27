pub type RawSector = [u8; 2352];
pub type DataSector = [u8; 2048];

pub trait Disc {
    fn read_sector(&mut self, lba: usize) -> Option<RawSector>;
}

pub fn sector_data(raw: RawSector) -> DataSector {
    let mut data = [0; _];
    data.copy_from_slice(&raw[24..][..2048]);

    data
}
