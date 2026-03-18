use arraydeque::{ArrayDeque, Wrapping};

use super::{
    Bus, Registers,
    ins::{LoadKind, OpResult, StoreKind},
};

/// Simplified and inaccurate implementation of MIPS pipeline.
#[derive(Default, Debug)]
pub struct Pipeline {
    queue: ArrayDeque<Latch, 5, Wrapping>,
}

#[derive(Debug, Copy, Clone)]
enum Latch {
    Flushed,
    Fetched(u32),
    Decoded(OpResult),
    Executed {
        op: OpResult,
        link_addr: u32,
    },
    Memory {
        op: OpResult,
        link_addr: u32,
        read: u32,
    },
}

impl Pipeline {
    /// Fetch as it should be.
    /// Read an instruction and increment PC (program count).
    pub fn fetch(&mut self, regs: &mut Registers, bus: &Bus) {
        let ins = bus.read_word(regs.pc);
        self.queue.push_front(Latch::Fetched(ins));
        regs.pc = regs.pc.wrapping_add(4);
    }

    // TODO : fallible
    /// Decode and *possibly* evaluate.
    /// The correct behavior is to evaluate in EX (execute) stage,
    /// but to keep the pipeline simple it's done in ID.
    pub fn decode(&mut self, regs: &Registers) {
        let stages = [
            self.queue.get(4).copied(),
            self.queue.get(3).copied(),
            self.queue.get(2).copied(),
        ];
        if let Some(stage @ &mut Latch::Fetched(ins)) = self.queue.get_mut(1) {
            let mut regs = regs.clone();

            // Forwarding of arithmetic ops from the next stages into registers.
            // Register file isn't touched, because the next ops may fall, so pipeline will run
            // from the previous place (and with original registers).
            for latch in stages {
                let Some(
                    Latch::Decoded(op) | Latch::Executed { op, .. } | Latch::Memory { op, .. },
                ) = latch
                else {
                    continue;
                };
                match op {
                    OpResult::Alu { dest, res } => {
                        // TODO : overflow trap
                        regs.general[dest] = res.unwrap();
                        regs.general[0] = 0;
                    }
                    OpResult::MulDiv {
                        res: Some((hi, lo)),
                    } => {
                        regs.hi = hi;
                        regs.lo = lo;
                    }
                    _ => {}
                }
            }

            // TODO : don't set nop, instead bail an error or trap
            *stage = Latch::Decoded(OpResult::decode_and_evaluate(ins, &regs).unwrap_or_default());
        }
    }

    /// Execute jumps and branches.
    /// Used instead of emulating delay slot.
    pub fn execute(&mut self, regs: &mut Registers) {
        // To overcome borrow checker
        // Don't really want to split array into mutable parts
        let mut jump_addr = None;

        if let Some(stage @ &mut Latch::Decoded(op)) = self.queue.get_mut(2) {
            match op {
                OpResult::Jump { addr, .. } => {
                    jump_addr = Some(addr);
                }
                OpResult::Branch { addr, .. } => {
                    jump_addr = addr;
                }
                _ => {}
            }
            *stage = Latch::Executed {
                op,
                // Address of instruction to flush
                link_addr: regs.pc - 4,
            };
        }

        if let Some(addr) = jump_addr {
            // Flush pre-fetched, but keep decoded (so called delay slot)
            self.queue[0] = Latch::Flushed;
            regs.pc = addr;
        }
    }

    /// Operations w/ memory like load/store.
    /// Eliminate need for load-delay slot, immitating the real CPU pipeline.
    pub fn memory(&mut self, bus: &mut Bus) {
        if let Some(stage @ &mut Latch::Executed { op, link_addr }) = self.queue.get_mut(3) {
            let mut read = 0;
            match op {
                OpResult::Load { addr, kind, .. } => {
                    read = match kind {
                        LoadKind::IByte => bus.read_byte(addr).cast_signed() as u32,
                        LoadKind::IHalf => bus.read_half(addr).cast_signed() as u32,
                        LoadKind::UByte => bus.read_byte(addr) as u32,
                        LoadKind::UHalf => bus.read_half(addr) as u32,
                        LoadKind::Word => bus.read_word(addr),
                        LoadKind::WordLeft => todo!(),
                        LoadKind::WordRight => todo!(),
                    };
                }
                OpResult::Store { addr, kind } => match kind {
                    StoreKind::Byte(val) => {
                        bus.store_byte(addr, val);
                    }
                    StoreKind::Half(val) => {
                        bus.store_half(addr, val);
                    }
                    StoreKind::Word(val) => {
                        bus.store_word(addr, val);
                    }
                    StoreKind::WordLeft(_) => todo!(),
                    StoreKind::WordRight(_) => todo!(),
                },
                _ => {}
            }
            *stage = Latch::Memory {
                op,
                link_addr,
                read,
            };
        }
    }

    /// Write to register file.
    /// Result of ALU ops are written in ID emulating forwarding of ops.
    pub fn writeback(&mut self, regs: &mut Registers) {
        if let Some(&mut Latch::Memory {
            op,
            link_addr,
            read,
        }) = self.queue.get_mut(4)
        {
            match op {
                OpResult::Alu { dest, res } => {
                    // No overflow, because checked already
                    regs.general[dest] = res.unwrap();
                }
                OpResult::Load { dest, .. } => {
                    regs.general[dest] = read;
                }
                OpResult::Branch { link: true, .. } => {
                    regs.general[31] = link_addr;
                }
                OpResult::Jump {
                    link: true,
                    link_reg,
                    ..
                } => {
                    regs.general[link_reg] = link_addr;
                }
                OpResult::MulDiv {
                    res: Some((hi, lo)),
                } => {
                    regs.hi = hi;
                    regs.lo = lo;
                }
                _ => {}
            }

            // Always zero
            regs.general[0] = 0;
        }
    }
}
