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

pub fn do_manual(
    bus: &mut Bus,
    ch: usize,
    chan: &mut Channel,
    ram_touched: &mut impl FnMut(u32),
) -> u64 {
    let mut cycles = 0u64;

    let step = match chan.chcr.step() {
        Step::Increment => 4,
        Step::Decrement => -4,
    };

    for words_left in (0..chan.bcr.word_count()).rev() {
        // DMA has its own addr translation
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
                    store_direct_ram(bus, addr, word, ram_touched);

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

pub fn do_block(
    bus: &mut Bus,
    ch: usize,
    chan: &mut Channel,
    ram_touched: &mut impl FnMut(u32),
) -> u64 {
    let mut cycles = 0u64;

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
                        let word = load_direct_ram(bus, addr);
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

    let mut cycles = 0u64;
    loop {
        let mut addr = chan.madr & 0x1FFFFC;

        let header = load_direct_ram(bus, addr);
        let next = header & 0xFFFFFF;
        let size = header >> 24;
        for _ in 0..size {
            addr = addr.wrapping_add(4) & 0x1FFFFC;

            let command = load_direct_ram(bus, addr);
            bus.gpu.dispatch_gp0(command);

            cycles = cycles.saturating_add(TIMINGS[GPU]);
        }

        if next == 0xFFFFFF {
            return cycles;
        }

        chan.madr = next;
    }
}

fn load_direct_ram(bus: &mut Bus, paddr: u32) -> u32 {
    let mut buf = [0; 4];
    buf.copy_from_slice(&bus.ram[paddr as usize..][..4]);
    u32::from_le_bytes(buf)
}

fn store_direct_ram(bus: &mut Bus, paddr: u32, value: u32, ram_touched: &mut impl FnMut(u32)) {
    bus.ram[paddr as usize..][..4].copy_from_slice(&value.to_le_bytes());
    ram_touched(paddr);
}
