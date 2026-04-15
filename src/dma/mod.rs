use strum::EnumCount;

use crate::{
    devices::{
        dma::{Channel, Direction, Port, Step, SyncMode},
        int::InterruptFlags,
    },
    interconnect::Bus,
};

#[derive(Debug, Default)]
pub struct Dma {
    working: [bool; Port::COUNT],
}

impl Dma {
    // TODO : count budget/cycles spent (k * [word transfered])
    pub fn run(&mut self, bus: &mut Bus) {
        // Find hi-prio enabled channel or any channel in work.
        let Some(ch) = bus.dma_ctrl.pick_highest_prio_chan().or_else(|| {
            self.working
                .iter()
                .position(|working| *working)
                .and_then(Port::from_repr)
        }) else {
            return;
        };

        self.working[ch as usize] = true;

        let mut chan = bus.dma_ctrl.channels[ch as usize];
        chan.start();

        match chan.chcr.sync_mode() {
            SyncMode::Manual => self.transfer_word(bus, ch, &mut chan),
            SyncMode::Request => self.transfer_block(bus, ch, &mut chan),
            SyncMode::LinkedList => self.transfer_ll(bus, ch),
            SyncMode::Reserved => unreachable!(),
        }

        if chan.try_finish() {
            self.working[ch as usize] = false;

            bus.dma_ctrl.dicr.set_irq_lane(ch);

            if bus.dma_ctrl.dicr.irq_signal() {
                bus.int_ctrl.raise(InterruptFlags::DMA);
            }
        }

        bus.dma_ctrl.channels[ch as usize] = chan;
    }

    fn transfer_word(&mut self, bus: &mut Bus, ch: Port, chan: &mut Channel) {
        let mut word_count = chan.bcr.word_count();

        let step = match chan.chcr.step() {
            Step::Increment => 4,
            Step::Decrement => -4,
        };
        if word_count > 0 {
            let addr = chan.madr & 0x1FFFFC;
            match chan.chcr.direction() {
                Direction::FromRam => todo!(),
                Direction::ToRam => match ch {
                    Port::Otc => {
                        let word = if word_count == 1 {
                            // Terminator for table
                            0xFFFFFF
                        } else {
                            chan.madr.wrapping_sub(4) & 0x1FFFFF
                        };

                        // TODO : don't silence error
                        let _ = bus.store::<4>(addr, word.to_le_bytes());
                    }
                    _ => todo!(),
                },
            }

            chan.madr = chan.madr.wrapping_add_signed(step);

            word_count -= 1;
        }

        chan.bcr.set_word_count(word_count);
    }

    fn transfer_block(&mut self, bus: &mut Bus, ch: Port, chan: &mut Channel) {
        let mut block_count = chan.bcr.block_count();

        let step = match chan.chcr.step() {
            Step::Increment => 4,
            Step::Decrement => -4,
        };
        if block_count > 0 {
            for _ in 0..chan.bcr.word_count() {
                let addr = chan.madr & 0x1FFFFC;

                // TODO : From/ToRam
                let word = bus.load::<4>(addr);

                chan.madr = chan.madr.wrapping_add_signed(step);
            }

            block_count -= 1;
        }

        chan.bcr.set_block_count(block_count);
    }

    fn transfer_ll(&mut self, bus: &mut Bus, ch: Port) {
        todo!()
    }
}
