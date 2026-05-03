use crate::interconnect::Bus;

use super::{Channel, Direction, Step};

const GPU: usize = 2;
const OTC: usize = 6;

pub fn do_manual(bus: &mut Bus, ch: usize, chan: &mut Channel) {
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
                }
                _ => todo!(),
            },
        }

        chan.madr = chan.madr.wrapping_add_signed(step);
    }

    chan.bcr.set_word_count(0);
}

pub fn do_block(bus: &mut Bus, ch: usize, chan: &mut Channel) {
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
                                return;
                            }
                        };
                        let word = u32::from_le_bytes(word);

                        bus.gpu.dispatch_gp0(word);
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
}

pub fn do_linked_list(bus: &mut Bus, ch: usize, chan: &mut Channel) {
    debug_assert_eq!(ch, GPU);

    loop {
        let mut addr = chan.madr & 0x1FFFFC;

        let header = match bus.load(addr) {
            Ok(res) => res,
            Err(err) => {
                tracing::warn!(?err, %addr, "DMA LinkedList load header error");

                chan.madr = 0xFFFFFF;
                return;
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
                    return;
                }
            };
            let command = u32::from_le_bytes(command);

            bus.gpu.dispatch_gp0(command);
        }

        if header & 0x800000 != 0 {
            chan.madr = 0xFFFFFF;
            return;
        }

        chan.madr = header & 0x1FFFFC;
    }
}
