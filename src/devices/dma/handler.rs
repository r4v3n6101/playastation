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
                        tracing::warn!(?err, %addr, %word, "OTC DMA manual error");
                    } else {
                        tracing::trace!(%addr, %word, "OTC DMA store");
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
                                tracing::warn!(?err, %addr, "RAM->GPU DMA block load word error");
                                return;
                            }
                        };
                        let word = u32::from_le_bytes(word);

                        tracing::trace!(%addr, %word, "RAM->GPU DMA block data word");
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
                tracing::warn!(?err, %addr, "DMA LinkedList header error");

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
                    tracing::warn!(?err, %addr, "DMA LinkedList command error");

                    chan.madr = 0xFFFFFF;
                    return;
                }
            };
            let command = u32::from_le_bytes(command);

            tracing::trace!(%addr, %command, "DMA LinkedList command");
            bus.gpu.dispatch_gp0(command);
        }

        if header & 0x800000 != 0 {
            tracing::trace!("DMA LinkedList end");

            chan.madr = 0xFFFFFF;
            return;
        }

        chan.madr = header & 0x1FFFFC;
        tracing::trace!(addr=%chan.madr, "DMA LinkedList next packet");
    }
}
