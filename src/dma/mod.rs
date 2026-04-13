use crate::{devices::int::InterruptFlags, interconnect::Bus};

pub struct Dma;

impl Dma {
    pub fn cycle(&mut self, bus: &mut Bus) {
        let Some(ch) = bus.dma_ctrl.pick_highest_prio_chan() else {
            return;
        };

        let chan = &mut bus.dma_ctrl.channels[ch];
        chan.start();

        // TODO : handle transfer

        if chan.try_finish() {
            bus.dma_ctrl.dicr.set_irq_lane(ch);

            if bus.dma_ctrl.dicr.irq_signal() {
                bus.int_ctrl.raise(InterruptFlags::DMA);
            }
        }
    }
}
