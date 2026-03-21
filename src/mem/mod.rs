const MEMSIZE: usize = 2 * 1024 * 1024;

#[derive(Debug)]
pub struct Error {
    pub bad_vaddr: u32,
    pub kind: ErrorKind,
}

#[derive(Debug)]
pub enum ErrorKind {
    UnalignedAddr,
    OutOfMemory,
}

pub struct Memory {
    pub storage: Box<[u8]>,
}

#[derive(Default)]
pub struct Bus {
    // TODO
    pub mem: Memory,
}

impl Default for Memory {
    fn default() -> Self {
        Self {
            storage: vec![0; MEMSIZE].into_boxed_slice(),
        }
    }
}

impl Bus {
    pub fn read_byte(&self, addr: u32) -> Result<u8, Error> {
        self.mem.storage.get(addr as usize).copied().ok_or(Error {
            bad_vaddr: addr,
            kind: ErrorKind::OutOfMemory,
        })
    }

    pub fn read_half(&self, addr: u32) -> Result<u16, Error> {
        if !addr.is_multiple_of(2) {
            return Err(Error {
                bad_vaddr: addr,
                kind: ErrorKind::UnalignedAddr,
            });
        }

        let bytes = self
            .mem
            .storage
            .get(addr as usize..(addr + 2) as usize)
            .ok_or(Error {
                bad_vaddr: addr,
                kind: ErrorKind::OutOfMemory,
            })?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub fn read_word(&self, addr: u32) -> Result<u32, Error> {
        if !addr.is_multiple_of(4) {
            return Err(Error {
                bad_vaddr: addr,
                kind: ErrorKind::UnalignedAddr,
            });
        }

        let bytes = self
            .mem
            .storage
            .get(addr as usize..(addr + 4) as usize)
            .ok_or(Error {
                bad_vaddr: addr,
                kind: ErrorKind::OutOfMemory,
            })?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn store_byte(&mut self, addr: u32, value: u8) -> Result<(), Error> {
        *self.mem.storage.get_mut(addr as usize).ok_or(Error {
            bad_vaddr: addr,
            kind: ErrorKind::OutOfMemory,
        })? = value;

        Ok(())
    }

    pub fn store_half(&mut self, addr: u32, value: u16) -> Result<(), Error> {
        if !addr.is_multiple_of(2) {
            return Err(Error {
                bad_vaddr: addr,
                kind: ErrorKind::UnalignedAddr,
            });
        }

        let bytes = self
            .mem
            .storage
            .get_mut(addr as usize..(addr + 2) as usize)
            .ok_or(Error {
                bad_vaddr: addr,
                kind: ErrorKind::OutOfMemory,
            })?;

        let [a, b] = value.to_le_bytes();
        bytes[0] = a;
        bytes[1] = b;

        Ok(())
    }

    pub fn store_word(&mut self, addr: u32, value: u32) -> Result<(), Error> {
        if !addr.is_multiple_of(4) {
            return Err(Error {
                bad_vaddr: addr,
                kind: ErrorKind::UnalignedAddr,
            });
        }

        let bytes = self
            .mem
            .storage
            .get_mut(addr as usize..(addr + 4) as usize)
            .ok_or(Error {
                bad_vaddr: addr,
                kind: ErrorKind::OutOfMemory,
            })?;

        let [a, b, c, d] = value.to_le_bytes();
        bytes[0] = a;
        bytes[1] = b;
        bytes[2] = c;
        bytes[3] = d;

        Ok(())
    }
}
