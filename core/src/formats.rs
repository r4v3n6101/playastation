use std::ops::Deref;

use yoke::Yoke;
use zerocopy::{SplitAt, TryFromBytes, little_endian::U32};
use zerocopy_derive::{FromZeros, Immutable, KnownLayout, SplitAt};

#[derive(Debug)]
pub struct BoxedExeFile(Yoke<&'static ExeFile, Box<[u8]>>);

#[derive(FromZeros, KnownLayout, SplitAt, Immutable, Debug)]
#[repr(C)]
pub struct ExeFile {
    pub header: ExeHeader,
    pub prog: [u8],
}

#[derive(FromZeros, KnownLayout, Immutable, Debug, Clone, Copy)]
#[repr(C)]
pub struct ExeHeader {
    /// ASCII magic "PS-X EXE".
    pub magic: [u8; 16],
    /// Initial PC.
    pub ipc: U32,
    /// Initial GPR-28.
    pub igp: U32,
    /// Destination in RAM to load to.
    pub ram_dest: U32,
    /// File size aligned to 2KiB
    pub file_size: U32,
    __unknown_20: U32,
    __unknown_24: U32,
    /// Memfill start address.
    pub mem_fill_start: U32,
    /// Memfill size.
    pub mem_fill_size: U32,
    /// Initial GPR-29 aka SP.
    pub ispb: U32,
    /// Initial GPR-29 SP offset.
    pub ispoff: U32,
    /// "Sony Computer Entertainment Inc." and so on.
    pub text: [u8; 1992],
}

impl Deref for BoxedExeFile {
    type Target = ExeFile;

    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}

impl BoxedExeFile {
    pub fn new(buf: Box<[u8]>) -> Self {
        Self(Yoke::attach_to_cart(buf, |bytes| {
            let file = ExeFile::try_ref_from_bytes(bytes).expect("2KiB of size at least");

            let split = file
                .split_at(file.header.file_size.get() as usize)
                .expect("at least `filesize` bytes");

            split.via_immutable().0
        }))
    }
}
