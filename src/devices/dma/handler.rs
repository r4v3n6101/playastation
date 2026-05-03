use crate::interconnect::Bus;

use super::{CHANNELS, Channel, Direction, Step};

const GPU: usize = 2;
const OTC: usize = 6;

/// Approximate timings of word transfer.
///
/// MdecIn: 0x110 clks per 0x100 words (1 cycle/word).
/// MdecOut: 0x110 clks per 0x100 words (1 cycle/word).
/// GPU: 0x110 clks per 0x100 words (1 cycle/word).
/// CDROM/BIOS: 0x1800 clks per 0x100 words (24 cycles/word).
/// CDROM/Games: 0x2800 clks per 0x100 words (40 cycles/word).
/// SPU: 0x420 clks per 0x100 (4 cycles/word).
/// PIO: 0x1400 clks per 0x100 (20 cycles/word).
/// OTC: 0x110 clks per 0x100 words (1 cycle/word).
const TIMINGS: [u64; CHANNELS] = [1, 1, 1, 30, 4, 20, 1];

pub fn do_manual(bus: &mut Bus, ch: usize, chan: &mut Channel) -> u64 {
    let mut cycles = 0u64;

    let step = match chan.chcr.step() {
        Step::Increment => 4,
        Step::Decrement => -4,
    };

    for words_left in (0..chan.bcr.word_count()).rev() {
        let addr = chan.madr & 0x1FFFFC;
        match chan.chcr.direction() {
            Direction::FromRam => todo!(),
            Direction::ToRam => match ch {
                OTC => {
                    let word = if words_left == 0 {
                        // Terminator for table
                        0xFFFFFF
                    } else {
                        addr.wrapping_sub(4)
                    };

                    // Silently stores, ignoring errors
                    if let Err(err) = bus.store::<4>(addr, word.to_le_bytes()) {
                        tracing::warn!(?err, %addr, %word, "OTC DMA store error");
                    }

                    cycles = cycles.saturating_add(TIMINGS[OTC]);
                }
                _ => todo!(),
            },
        }

        chan.madr = chan.madr.wrapping_add_signed(step);
    }

    chan.bcr.set_word_count(0);

    cycles
}

pub fn do_block(bus: &mut Bus, ch: usize, chan: &mut Channel) -> u64 {
    let mut cycles = 0;

    let step = match chan.chcr.step() {
        Step::Increment => 4,
        Step::Decrement => -4,
    };

    for _ in 0..chan.bcr.block_count() {
        for _ in 0..chan.bcr.word_count() {
            let addr = chan.madr & 0x1FFFFC;

            match chan.chcr.direction() {
                Direction::FromRam => match ch {
                    GPU => {
                        let word = match bus.load::<4>(addr) {
                            Ok(res) => res,
                            Err(err) => {
                                tracing::warn!(?err, %addr, "RAM->GPU DMA block load error");
                                return cycles;
                            }
                        };
                        let word = u32::from_le_bytes(word);

                        bus.gpu.dispatch_gp0(word);
                        cycles = cycles.saturating_add(TIMINGS[GPU]);
                    }
                    _ => todo!(),
                },
                Direction::ToRam => todo!(),
            }

            chan.madr = chan.madr.wrapping_add_signed(step);
        }
    }

    chan.bcr.set_word_count(0);
    chan.bcr.set_block_count(0);

    cycles
}

pub fn do_linked_list(bus: &mut Bus, ch: usize, chan: &mut Channel) -> u64 {
    debug_assert_eq!(ch, GPU);

    let mut cycles = 0;
    loop {
        let mut addr = chan.madr & 0x1FFFFC;

        let header = match bus.load(addr) {
            Ok(res) => res,
            Err(err) => {
                tracing::warn!(?err, %addr, "DMA LinkedList load header error");

                chan.madr = 0xFFFFFF;
                return cycles;
            }
        };

        let header = u32::from_le_bytes(header);
        let size = header >> 24;
        for _ in 0..size {
            addr = addr.wrapping_add(4);

            let command = match bus.load(addr) {
                Ok(res) => res,
                Err(err) => {
                    tracing::warn!(?err, %addr, "DMA LinkedList load command error");

                    chan.madr = 0xFFFFFF;
                    return cycles;
                }
            };
            let command = u32::from_le_bytes(command);

            bus.gpu.dispatch_gp0(command);
            cycles = cycles.saturating_add(TIMINGS[GPU]);
        }

        if header & 0x800000 != 0 {
            chan.madr = 0xFFFFFF;
            return cycles;
        }

        chan.madr = header & 0x1FFFFC;
    }
}
