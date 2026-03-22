use arraydeque::{ArrayDeque, Wrapping};

use crate::mem;

use super::{
    Registers,
    cop0::Cop0,
    ins::{LoadKind, OpResult, StoreKind},
};

#[derive(Debug)]
pub struct Error {
    pub pc: u32,
    pub kind: ErrorKind,
}

#[derive(Debug)]
pub enum ErrorKind {
    AluOverflow,
    InvalidInstruction(u32),
    InsLoad(mem::Error),
    MemoryLoad(mem::Error),
    MemoryStore(mem::Error),
    Break,
    Syscall,
    Interrupt,
}

/// Simplified and inaccurate implementation of MIPS pipeline.
#[derive(Debug)]
pub struct Pipeline {
    queue: ArrayDeque<Latch, 5, Wrapping>,
}

#[derive(Debug, Copy, Clone)]
enum Latch {
    Flushed,
    Fetched { pc: u32, ins: u32 },
    Decoded { pc: u32, op: OpResult },
    Executed { pc: u32, op: OpResult },
    Memory { pc: u32, op: OpResult, read: u32 },
    Writeback { pc: u32, op: OpResult },
}

impl Default for Pipeline {
    fn default() -> Self {
        Self {
            queue: ArrayDeque::from([Latch::Flushed; 5]),
        }
    }
}

impl Pipeline {
    /// Fetch. Read an instruction and increment PC (program count).
    /// May be interrupted from the outside.
    pub fn fetch(&mut self, pc: &mut u32, cop0: &Cop0, bus: &mem::Bus) -> Result<(), Error> {
        // Here for simplicity, will be handled as an exception (IF's PC is saved to EPC)
        if cop0.interrupt_pending() {
            return Err(Error {
                pc: *pc,
                kind: ErrorKind::Interrupt,
            });
        }

        self.queue.push_front(Latch::Fetched {
            pc: *pc,
            ins: bus.read_word(*pc).map_err(|err| {
                let pc = *pc;
                Error {
                    pc,
                    kind: ErrorKind::InsLoad(err),
                }
            })?,
        });
        *pc = pc.wrapping_add(4);

        Ok(())
    }

    /// Decode and *possibly* evaluate.
    /// The correct behavior is to evaluate in EX (execute) stage,
    /// but to keep the pipeline simple and reduce additional if/matches it's done in ID.
    pub fn decode(&mut self, regs: &Registers, cop0: &mut Cop0) -> Result<(), Error> {
        let stages = [
            self.queue.get(4).copied(),
            self.queue.get(3).copied(),
            self.queue.get(2).copied(),
        ];
        if let stage @ &mut Latch::Fetched { pc, ins } = &mut self.queue[1] {
            let mut regs = regs.clone();

            // Forwarding of arithmetic ops from the next stages into registers.
            // Register file isn't touched, because the next ops may fall, so pipeline will run
            // from the previous place (and with original registers).
            for latch in stages {
                let Some(
                    Latch::Decoded { op, .. }
                    | Latch::Executed { op, .. }
                    | Latch::Memory { op, .. },
                ) = latch
                else {
                    continue;
                };
                match op {
                    OpResult::Alu { dest, res } => {
                        // Forwarded results are already checked
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

            let op = OpResult::decode_and_evaluate(ins, &regs).ok_or(Error {
                pc,
                kind: ErrorKind::InvalidInstruction(ins),
            })?;

            match op {
                OpResult::Alu { res: None, .. } => {
                    return Err(Error {
                        pc,
                        kind: ErrorKind::AluOverflow,
                    });
                }
                OpResult::Break => {
                    return Err(Error {
                        pc,
                        kind: ErrorKind::Break,
                    });
                }
                OpResult::Syscall => {
                    return Err(Error {
                        pc,
                        kind: ErrorKind::Syscall,
                    });
                }
                OpResult::Rfe => {
                    cop0.exception_leave();
                }
                _ => (),
            }

            *stage = Latch::Decoded { pc, op };
        }

        Ok(())
    }

    /// Execute jumps and branches.
    /// Used instead of emulating delay slot.
    pub fn execute(&mut self, next_pc: &mut u32) {
        // To overcome borrow checker
        // Don't really want to split array into mutable parts
        let mut jump_addr = None;

        if let stage @ &mut Latch::Decoded { pc, op } = &mut self.queue[2] {
            match op {
                OpResult::Jump { addr, .. } => {
                    jump_addr = Some(addr);
                }
                OpResult::Branch { addr, .. } => {
                    jump_addr = addr;
                }
                _ => {}
            }
            *stage = Latch::Executed { pc, op };
        }

        if let Some(addr) = jump_addr {
            // Flush pre-fetched, but keep decoded (so called delay slot)
            self.queue[0] = Latch::Flushed;
            *next_pc = addr;
        }
    }

    /// Operations w/ memory like load/store.
    /// Eliminate need for load-delay slot, immitating the real CPU pipeline.
    pub fn memory(&mut self, bus: &mut mem::Bus) -> Result<(), Error> {
        if let stage @ &mut Latch::Executed { pc, op } = &mut self.queue[3] {
            let mut read = 0;
            match op {
                OpResult::Load { addr, kind, .. } => {
                    read = match kind {
                        LoadKind::IByte => bus
                            .read_byte(addr)
                            .map_err(|err| Error {
                                pc,
                                kind: ErrorKind::MemoryLoad(err),
                            })?
                            .cast_signed() as u32,
                        LoadKind::IHalf => bus
                            .read_half(addr)
                            .map_err(|err| Error {
                                pc,
                                kind: ErrorKind::MemoryLoad(err),
                            })?
                            .cast_signed() as u32,
                        LoadKind::UByte => u32::from(bus.read_byte(addr).map_err(|err| Error {
                            pc,
                            kind: ErrorKind::MemoryLoad(err),
                        })?),
                        LoadKind::UHalf => u32::from(bus.read_half(addr).map_err(|err| Error {
                            pc,
                            kind: ErrorKind::MemoryLoad(err),
                        })?),
                        LoadKind::Word => bus.read_word(addr).map_err(|err| Error {
                            pc,
                            kind: ErrorKind::MemoryLoad(err),
                        })?,
                        LoadKind::WordLeft => todo!(),
                        LoadKind::WordRight => todo!(),
                    };
                }
                OpResult::Store { addr, kind } => match kind {
                    StoreKind::Byte(val) => {
                        bus.store_byte(addr, val).map_err(|err| Error {
                            pc,
                            kind: ErrorKind::MemoryStore(err),
                        })?;
                    }
                    StoreKind::Half(val) => {
                        bus.store_half(addr, val).map_err(|err| Error {
                            pc,
                            kind: ErrorKind::MemoryStore(err),
                        })?;
                    }
                    StoreKind::Word(val) => {
                        bus.store_word(addr, val).map_err(|err| Error {
                            pc,
                            kind: ErrorKind::MemoryStore(err),
                        })?;
                    }
                    StoreKind::WordLeft(_) => todo!(),
                    StoreKind::WordRight(_) => todo!(),
                },
                _ => {}
            }
            *stage = Latch::Memory { pc, op, read };
        }

        Ok(())
    }

    /// Write to register file.
    /// Result of ALU ops are written in ID emulating forwarding of ops.
    pub fn writeback(&mut self, regs: &mut Registers) {
        if let stage @ &mut Latch::Memory { pc, op, read } = &mut self.queue[4] {
            match op {
                OpResult::Alu { dest, res } => {
                    // No overflow, because checked already
                    regs.general[dest] = res.unwrap();
                }
                OpResult::Load { dest, .. } => {
                    regs.general[dest] = read;
                }
                OpResult::Branch { link: true, .. } => {
                    regs.general[31] = pc + 8;
                }
                OpResult::Jump {
                    link: true,
                    link_reg,
                    ..
                } => {
                    regs.general[link_reg] = pc + 8;
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

            *stage = Latch::Writeback { pc, op };
        }
    }

    /// Flush the pipeline with [`count`] of instructions.
    /// If the next stage is branch or jump, flush it too.
    /// Returns PC of parent branch/jump or [`None`]
    // TODO : I don't like this return
    pub fn flush(&mut self, count: usize) -> Option<u32> {
        for i in 0..count {
            self.queue[i] = Latch::Flushed;
        }

        // Flush branch/jump operation older than failed delay slot and save its PC
        let mut parent = None;
        if let (1, stage @ &mut Latch::Decoded { pc, op, .. })
        | (2, stage @ &mut Latch::Executed { pc, op, .. })
        | (4, stage @ &mut Latch::Writeback { pc, op }) = (count, &mut self.queue[count])
            && op.has_delay_slot()
        {
            parent = Some(pc);
            *stage = Latch::Flushed;
        }

        parent
    }
}
