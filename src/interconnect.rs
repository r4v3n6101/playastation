use std::ops::Range;

use bytes::BytesMut;

use crate::devices::{Mmio, dma::DmaController, int::InterruptController};

// MIPS uses segmented memory, but PSX ignore them and treat all segments as mirror to each other
const KUSEG: Range<u32> = 0x0000_0000..0x7FFF_FFFF;
const KSEG0: Range<u32> = 0x8000_0000..0x9FFF_FFFF;
const KSEG1: Range<u32> = 0xA000_0000..0xBFFF_FFFF;
const CACHE_CONTROL: u32 = 0xFFFE_0130;

// Mapped memory
const RAM: Range<u32> = 0x0000_0000..0x001F_FFFF;
const EXPANSION1: Range<u32> = 0x1F00_0000..0x1F7F_FFFF;
const SCRATCHPAD: Range<u32> = 0x1F80_0000..0x1F80_03FF;

// Hardware registers regions
const HW_REGS: Range<u32> = 0x1F801000..0x1F801FFF;
const INT_CTRL: Range<u32> = 0x1F801070..0x1F801078;
const DMA_CTRL: Range<u32> = 0x1F801080..0x1F8010FF;

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
    pub dma_ctrl: DmaController,
}

impl Default for Bus {
    fn default() -> Self {
        let mut buf = BytesMut::zeroed(
            RAM.len()
                + 1
                + EXPANSION1.len()
                + 1
                + SCRATCHPAD.len()
                + 1
                + MISC.len()
                + 1
                + EXPANSION2.len()
                + 1
                + BIOS.len()
                + 1,
        );

        let bios = buf.split_to(BIOS.len() + 1);
        let ram = buf.split_to(RAM.len() + 1);
        let misc = buf.split_to(MISC.len() + 1);
        let scratchpad = buf.split_to(SCRATCHPAD.len() + 1);
        let expansion1 = buf.split_to(EXPANSION1.len() + 1);
        let expansion2 = buf.split_to(EXPANSION2.len() + 1);

        Self {
            bios,
            ram,
            misc,
            scratchpad,
            expansion1,
            expansion2,

            int_ctrl: Default::default(),
            dma_ctrl: Default::default(),
        }
    }
}

impl Bus {
    pub fn load<const N: usize>(&self, addr: u32) -> Result<[u8; N], BusError> {
        if !addr.is_multiple_of(N as u32) {
            return Err(BusError {
                bad_vaddr: addr,
                kind: BusErrorKind::UnalignedAddr,
            });
        }

        let mut bytes = [0; N];

        // TODO : cache control 0xFFFE0130
        match translate_addr(addr)? {
            x if RAM.contains(&x) => {
                bytes.copy_from_slice(&self.ram[(x - RAM.start) as usize..][..N]);
            }
            x if EXPANSION1.contains(&x) => {
                bytes.copy_from_slice(&self.expansion1[(x - EXPANSION1.start) as usize..][..N]);
            }
            x if SCRATCHPAD.contains(&x) => {
                bytes.copy_from_slice(&self.scratchpad[(x - SCRATCHPAD.start) as usize..][..N]);
            }
            x if MISC.contains(&x) => {
                bytes.copy_from_slice(&self.misc[(x - MISC.start) as usize..][..N]);
            }
            x if EXPANSION2.contains(&x) => {
                bytes.copy_from_slice(&self.expansion2[(x - EXPANSION2.start) as usize..][..N]);
            }
            x if BIOS.contains(&x) => {
                bytes.copy_from_slice(&self.bios[(x - BIOS.start) as usize..][..N]);
            }
            x if INT_CTRL.contains(&x) => {
                self.int_ctrl.read(&mut bytes, x - INT_CTRL.start);
            }
            x if DMA_CTRL.contains(&x) => {
                self.dma_ctrl.read(&mut bytes, x - DMA_CTRL.start);
            }
            x if HW_REGS.contains(&x) => {
                if x == 0x1F801814 || addr == 0x1F801814 {
                    panic!("GPUSTAT");
                }
            }
            _ => {
                return Err(BusError {
                    bad_vaddr: addr,
                    kind: BusErrorKind::Unmapped,
                });
            }
        }

        Ok(bytes)
    }

    pub fn store<const N: usize>(&mut self, addr: u32, value: [u8; N]) -> Result<(), BusError> {
        if !addr.is_multiple_of(N as u32) {
            return Err(BusError {
                bad_vaddr: addr,
                kind: BusErrorKind::UnalignedAddr,
            });
        }

        match translate_addr(addr)? {
            x if RAM.contains(&x) => {
                self.ram[(x - RAM.start) as usize..][..N].copy_from_slice(&value);
            }
            x if EXPANSION1.contains(&x) => {
                self.expansion1[(x - EXPANSION1.start) as usize..][..N].copy_from_slice(&value);
            }
            x if SCRATCHPAD.contains(&x) => {
                self.scratchpad[(x - SCRATCHPAD.start) as usize..][..N].copy_from_slice(&value);
            }
            x if MISC.contains(&x) => {
                self.misc[(x - MISC.start) as usize..][..N].copy_from_slice(&value);
            }
            x if EXPANSION2.contains(&x) => {
                self.expansion2[(x - EXPANSION2.start) as usize..][..N].copy_from_slice(&value);
            }
            x if BIOS.contains(&x) => {
                self.bios[(x - BIOS.start) as usize..][..N].copy_from_slice(&value);
            }
            x if INT_CTRL.contains(&x) => {
                self.int_ctrl.write(x - INT_CTRL.start, &value);
            }
            x if DMA_CTRL.contains(&x) => {
                self.dma_ctrl.write(x - DMA_CTRL.start, &value);
            }
            x if HW_REGS.contains(&x) => {}
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
        CACHE_CONTROL => {
            // TODO
            Ok(0)
        }
        _ => Err(BusError {
            bad_vaddr: addr,
            kind: BusErrorKind::Unmapped,
        }),
    }
}
