use std::ops::Range;

use bytes::BytesMut;

use crate::devices::{Mmio, int::InterruptController};

// MIPS uses segmented memory, but PSX ignore them and treat all segments as mirror to each other
const KUSEG: Range<u32> = 0x0000_0000..0x7FFF_FFFF;
const KSEG0: Range<u32> = 0x8000_0000..0x9FFF_FFFF;
const KSEG1: Range<u32> = 0xA000_0000..0xBFFF_FFFF;

// Mapped memory
const RAM: Range<u32> = 0x0000_0000..0x001F_FFFF;
const EXPANSION1: Range<u32> = 0x1F00_0000..0x1F7F_FFFF;
const SCRATCHPAD: Range<u32> = 0x1F80_0000..0x1F80_03FF;

// Hardware registers regions
const INT_CTRL: Range<u32> = 0x1F801070..0x1F801078;

const MISC: Range<u32> = 0x1F80_2000..0x1F80_2FFF;
const EXPANSION2: Range<u32> = 0x1FA0_0000..0x1FA1_FFFF;
const BIOS: Range<u32> = 0x1FC0_0000..0x1FC7_FFFF;

#[derive(Debug)]
pub struct BusError {
    pub bad_vaddr: u32,
    pub kind: BusErrorKind,
}

#[derive(Debug)]
pub enum BusErrorKind {
    UnalignedAddr,
    Unmapped,
}

pub struct Bus {
    pub bios: BytesMut,
    pub ram: BytesMut,
    pub misc: BytesMut,
    pub scratchpad: BytesMut,
    pub expansion1: BytesMut,
    pub expansion2: BytesMut,

    // Devices
    pub int_ctrl: InterruptController,
}

impl Default for Bus {
    fn default() -> Self {
        let mut buf = BytesMut::zeroed(
            RAM.len()
                + EXPANSION1.len()
                + SCRATCHPAD.len()
                + MISC.len()
                + EXPANSION2.len()
                + BIOS.len(),
        );

        Self {
            bios: buf.split_to(BIOS.len()),
            ram: buf.split_to(RAM.len()),
            misc: buf.split_to(MISC.len()),
            scratchpad: buf.split_to(SCRATCHPAD.len()),
            expansion1: buf.split_to(EXPANSION1.len()),
            expansion2: buf.split_to(EXPANSION2.len()),

            int_ctrl: Default::default(),
        }
    }
}

impl Bus {
    pub fn read_byte(&self, addr: u32) -> Result<u8, BusError> {
        let mut bytes = [0; 1];
        self.memread(&mut bytes, addr)?;
        Ok(bytes[0])
    }

    pub fn read_half(&self, addr: u32) -> Result<u16, BusError> {
        if !addr.is_multiple_of(2) {
            return Err(BusError {
                bad_vaddr: addr,
                kind: BusErrorKind::UnalignedAddr,
            });
        }

        let mut bytes = [0; 2];
        self.memread(&mut bytes, addr)?;
        Ok(u16::from_le_bytes(bytes))
    }

    pub fn read_word(&self, addr: u32) -> Result<u32, BusError> {
        if !addr.is_multiple_of(4) {
            return Err(BusError {
                bad_vaddr: addr,
                kind: BusErrorKind::UnalignedAddr,
            });
        }

        let mut bytes = [0; 4];
        self.memread(&mut bytes, addr)?;
        Ok(u32::from_le_bytes(bytes))
    }

    pub fn store_byte(&mut self, addr: u32, value: u8) -> Result<(), BusError> {
        self.memwrite(addr, &value.to_le_bytes())
    }

    pub fn store_half(&mut self, addr: u32, value: u16) -> Result<(), BusError> {
        if !addr.is_multiple_of(2) {
            return Err(BusError {
                bad_vaddr: addr,
                kind: BusErrorKind::UnalignedAddr,
            });
        }

        self.memwrite(addr, &value.to_le_bytes())
    }

    pub fn store_word(&mut self, addr: u32, value: u32) -> Result<(), BusError> {
        if !addr.is_multiple_of(4) {
            return Err(BusError {
                bad_vaddr: addr,
                kind: BusErrorKind::UnalignedAddr,
            });
        }

        self.memwrite(addr, &value.to_le_bytes())
    }

    fn memread(&self, dest: &mut [u8], addr: u32) -> Result<(), BusError> {
        // TODO : cache control 0xFFFE0130
        match translate_addr(addr)? {
            x if RAM.contains(&x) => {
                dest.copy_from_slice(&self.ram[(x - RAM.start) as usize..][..dest.len()]);
            }
            x if EXPANSION1.contains(&x) => {
                dest.copy_from_slice(
                    &self.expansion1[(x - EXPANSION1.start) as usize..][..dest.len()],
                );
            }
            x if SCRATCHPAD.contains(&x) => {
                dest.copy_from_slice(
                    &self.scratchpad[(x - SCRATCHPAD.start) as usize..][..dest.len()],
                );
            }
            x if MISC.contains(&x) => {
                dest.copy_from_slice(&self.misc[(x - MISC.start) as usize..][..dest.len()]);
            }
            x if EXPANSION2.contains(&x) => {
                dest.copy_from_slice(
                    &self.expansion2[(x - EXPANSION2.start) as usize..][..dest.len()],
                );
            }
            x if BIOS.contains(&x) => {
                dest.copy_from_slice(&self.bios[(x - BIOS.start) as usize..][..dest.len()]);
            }
            x if INT_CTRL.contains(&x) => {
                self.int_ctrl.read(dest, x - INT_CTRL.start);
            }
            _ => {
                return Err(BusError {
                    bad_vaddr: addr,
                    kind: BusErrorKind::Unmapped,
                });
            }
        }

        Ok(())
    }

    fn memwrite(&mut self, addr: u32, value: &[u8]) -> Result<(), BusError> {
        match translate_addr(addr)? {
            x if RAM.contains(&x) => {
                self.ram[(x - RAM.start) as usize..][..value.len()].copy_from_slice(value);
            }
            x if EXPANSION1.contains(&x) => {
                self.expansion1[(x - EXPANSION1.start) as usize..][..value.len()]
                    .copy_from_slice(value);
            }
            x if SCRATCHPAD.contains(&x) => {
                self.scratchpad[(x - SCRATCHPAD.start) as usize..][..value.len()]
                    .copy_from_slice(value);
            }
            x if MISC.contains(&x) => {
                self.misc[(x - MISC.start) as usize..][..value.len()].copy_from_slice(value);
            }
            x if EXPANSION2.contains(&x) => {
                self.expansion2[(x - EXPANSION2.start) as usize..][..value.len()]
                    .copy_from_slice(value);
            }
            x if BIOS.contains(&x) => {
                self.bios[(x - BIOS.start) as usize..][..value.len()].copy_from_slice(value);
            }
            x if INT_CTRL.contains(&x) => {
                self.int_ctrl.write(x - INT_CTRL.start, value);
            }
            _ => {
                return Err(BusError {
                    bad_vaddr: addr,
                    kind: BusErrorKind::Unmapped,
                });
            }
        }

        Ok(())
    }
}

// Translate virtual address from segments into physical one.
fn translate_addr(addr: u32) -> Result<u32, BusError> {
    match addr {
        x if KUSEG.contains(&x) || KSEG0.contains(&x) || KSEG1.contains(&x) => {
            Ok(addr & 0x1FFF_FFFF)
        }
        _ => Err(BusError {
            bad_vaddr: addr,
            kind: BusErrorKind::Unmapped,
        }),
    }
}
