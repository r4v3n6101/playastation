use crate::{RAM_SIZE, interconnect::Bus};

use super::{CHANNELS, Channel, Direction, Step};

const MDEC_IN: usize = 0;
const MDEC_OUT: usize = 1;
const GPU: usize = 2;
const CDROM: usize = 3;
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
                CDROM => {
                    let word = u32::from_le_bytes([
                        bus.cdrom.pop_data().unwrap_or(0),
                        bus.cdrom.pop_data().unwrap_or(0),
                        bus.cdrom.pop_data().unwrap_or(0),
                        bus.cdrom.pop_data().unwrap_or(0),
                    ]);

                    // SAFETY: addr masked above
                    unsafe {
                        store_direct_ram(bus, addr, word, ram_touched);
                    }

                    cycles = cycles.saturating_add(TIMINGS[CDROM]);
                }
                OTC => {
                    let word = if words_left == 0 {
                        // Terminator for table
                        0xFFFFFF
                    } else {
                        addr.wrapping_sub(4)
                    };

                    // SAFETY: addr masked above
                    unsafe {
                        store_direct_ram(bus, addr, word, ram_touched);
                    }

                    cycles = cycles.saturating_add(TIMINGS[OTC]);
                }
                _ => todo!("{ch}={chan:#?}"),
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
                    MDEC_IN => {}
                    MDEC_OUT => {}
                    GPU => {
                        // SAFETY: addr masked above
                        let word = unsafe { load_direct_ram(bus, addr) };
                        bus.gpu.dispatch_gp0(word);

                        cycles = cycles.saturating_add(TIMINGS[GPU]);
                    }
                    _ => todo!("{ch}={chan:#?}"),
                },
                Direction::ToRam => match ch {
                    MDEC_IN => {}
                    MDEC_OUT => {}
                    GPU => {
                        let word = bus.gpu.gpuread();
                        // SAFETY: addr masked above
                        unsafe {
                            store_direct_ram(bus, addr, word, ram_touched);
                        }

                        cycles = cycles.saturating_add(TIMINGS[GPU]);
                    }
                    _ => todo!("{ch}={chan:#?}"),
                },
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

        // SAFETY: addr masked above
        let header = unsafe { load_direct_ram(bus, addr) };
        let next = header & 0xFFFFFF;
        let size = header >> 24;
        for _ in 0..size {
            addr = addr.wrapping_add(4) & 0x1FFFFC;

            // SAFETY: addr masked above
            let command = unsafe { load_direct_ram(bus, addr) };
            bus.gpu.dispatch_gp0(command);

            cycles = cycles.saturating_add(TIMINGS[GPU]);
        }

        if next == 0xFFFFFF {
            return cycles;
        }

        chan.madr = next;
    }
}

unsafe fn load_direct_ram(bus: &mut Bus, paddr: u32) -> u32 {
    debug_assert!(
        paddr <= RAM_SIZE as u32 && paddr.is_multiple_of(4),
        "unaligned RAM address"
    );

    let mut buf = [0; 4];
    unsafe { buf.copy_from_slice(bus.ram.get_unchecked(paddr as usize..).get_unchecked(..4)) }
    u32::from_le_bytes(buf)
}

unsafe fn store_direct_ram(
    bus: &mut Bus,
    paddr: u32,
    value: u32,
    ram_touched: &mut impl FnMut(u32),
) {
    debug_assert!(
        paddr <= RAM_SIZE as u32 && paddr.is_multiple_of(4),
        "unaligned RAM address"
    );

    unsafe {
        bus.ram
            .get_unchecked_mut(paddr as usize..)
            .get_unchecked_mut(..4)
            .copy_from_slice(&value.to_le_bytes());
    }
    ram_touched(paddr);
}
