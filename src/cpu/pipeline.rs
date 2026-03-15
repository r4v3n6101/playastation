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

#[derive(Debug)]
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
    /// but to keep the pipeline simple (w/o forwarding)
    /// results are immediately written to register file.
    pub fn decode(&mut self, regs: &mut Registers) {
        if let Some(stage @ &mut Latch::Fetched(ins)) = self.queue.get_mut(1) {
            // TODO : don't set nop, instead bail an error or trap
            let op = OpResult::decode_and_evaluate(ins, regs).unwrap_or_default();

            // This is kind of forwarding of Alu ops from EX/MEM stages
            // (but we can actually write its results in ID)
            match op {
                OpResult::Alu { dest, res } => {
                    // TODO : overflow trap
                    regs.general[dest] = res.unwrap();

                    // Always zero
                    regs.general[0] = 0;
                }
                OpResult::MulDiv {
                    res: Some((hi, lo)),
                } => {
                    regs.hi = hi;
                    regs.lo = lo;
                }
                _ => (),
            }
            *stage = Latch::Decoded(op);
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
                    StoreKind::WordLeft(val) => todo!(),
                    StoreKind::WordRight(val) => todo!(),
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
                _ => {}
            }

            // Always zero
            regs.general[0] = 0;
        }
    }
}
