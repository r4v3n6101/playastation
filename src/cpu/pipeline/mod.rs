use arraydeque::{ArrayDeque, Wrapping};
use ins::{ExecRes, LoadKind, Opcode, StoreKind};

use crate::mem;

use super::{Registers, cop0::Cop0};

mod ins;

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

#[derive(Debug)]
pub struct Pipeline {
    queue: ArrayDeque<Latch, 6, Wrapping>,
}

#[derive(Debug, Copy, Clone)]
enum Latch {
    Flushed,
    Fetched {
        pc: u32,
        ins: u32,
    },
    Decoded {
        pc: u32,
        ins: u32,
        opcode: Opcode,
        regs: Registers,
    },
    Executed {
        pc: u32,
        opcode: Opcode,
        exec: ExecRes,
    },
    Memory {
        pc: u32,
        opcode: Opcode,
        exec: ExecRes,
        read: u32,
    },
    WrittenBack {
        opcode: Opcode,
        exec: ExecRes,
    },
}

impl Default for Pipeline {
    fn default() -> Self {
        Self {
            queue: ArrayDeque::from([Latch::Flushed; 6]),
        }
    }
}

impl Pipeline {
    /// Fetch. Read an instruction and increment PC (program count).
    /// May be interrupted from the outside.
    pub fn fetch(&mut self, pc: &mut u32, cop0: &Cop0, bus: &mem::Bus) -> Result<(), Error> {
        // Here for simplicity, will be handled as an exception
        // (IF's PC is saved to EPC, or ID if branch)
        if cop0.interrupt_pending() {
            return Err(Error {
                pc: *pc,
                kind: ErrorKind::Interrupt,
            });
        }

        self.queue.push_front(Latch::Fetched {
            pc: *pc,
            ins: bus.read_word(*pc).map_err(|err| Error {
                pc: *pc,
                kind: ErrorKind::InsLoad(err),
            })?,
        });
        *pc = pc.wrapping_add(4);

        Ok(())
    }

    /// Decode operation and save snapshot of registers.
    pub fn decode(&mut self, regs: &Registers) -> Result<(), Error> {
        if let stage @ &mut Latch::Fetched { pc, ins } = &mut self.queue[1] {
            let opcode = Opcode::decode(ins).ok_or(Error {
                pc,
                kind: ErrorKind::InvalidInstruction(ins),
            })?;

            *stage = Latch::Decoded {
                pc,
                ins,
                opcode,
                regs: *regs,
            };
        }

        Ok(())
    }

    /// Execute jumps and branches.
    /// Used instead of emulating branch delay slot.
    pub fn execute(&mut self, next_pc: &mut u32, cop0: &mut Cop0) -> Result<(), Error> {
        // Don't really wanna mess with BC, so split array into mutable parts
        let mut jump_addr = None;

        let stages = [
            self.queue.get(5).copied(),
            self.queue.get(4).copied(),
            self.queue.get(3).copied(),
        ];
        if let stage @ &mut Latch::Decoded {
            pc,
            ins,
            opcode,
            mut regs,
        } = &mut self.queue[2]
        {
            // Forwarding of arithmetic ops from the next stages into registers.
            // Register file isn't touched, because pending ops (in MEM) may fail, so pipeline will run
            // from the previous place (and with original registers).
            for latch in stages {
                if let Some(
                    Latch::Executed { exec, .. }
                    | Latch::Memory { exec, .. }
                    | Latch::WrittenBack { exec, .. },
                ) = latch
                {
                    match exec {
                        // Forwarded results are already checked
                        ExecRes::Alu {
                            dest,
                            res: Some(res),
                        } => {
                            regs.general[dest] = res;
                        }
                        ExecRes::MulDiv { hi, lo } => {
                            regs.hi = hi;
                            regs.lo = lo;
                        }
                        // TODO : this must not be forwarded, but I don't know real semantic of
                        // this operation
                        ExecRes::Mfc0 { dest, from } => {
                            regs.general[dest] = cop0.regs[from];
                        }
                        _ => {}
                    }
                }

                // Always
                regs.general[0] = 0;
            }

            let exec = opcode.execute(ins, &regs);
            match exec {
                ExecRes::Jump { addr, .. } => {
                    jump_addr = Some(addr);
                }
                ExecRes::Branch { addr, .. } => {
                    jump_addr = addr;
                }
                ExecRes::Alu { res: None, .. } => {
                    return Err(Error {
                        pc,
                        kind: ErrorKind::AluOverflow,
                    });
                }
                ExecRes::Break => {
                    return Err(Error {
                        pc,
                        kind: ErrorKind::Break,
                    });
                }
                ExecRes::Syscall => {
                    return Err(Error {
                        pc,
                        kind: ErrorKind::Syscall,
                    });
                }
                ExecRes::Rfe => {
                    cop0.exception_leave();
                }
                _ => {}
            }

            *stage = Latch::Executed { pc, opcode, exec };
        }

        if let Some(addr) = jump_addr {
            // Flush pre-fetched, but keep decoded (so called delay slot)
            self.queue[0] = Latch::Flushed;
            *next_pc = addr;
        }

        Ok(())
    }

    /// Operations with memory like load/store. Eliminate need for load-delay slot.
    pub fn memory(&mut self, bus: &mut mem::Bus, cop0: &mut Cop0) -> Result<(), Error> {
        if let stage @ &mut Latch::Executed { pc, opcode, exec } = &mut self.queue[3] {
            let mut read = 0;
            match exec {
                ExecRes::Load { addr, kind, .. } => {
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
                ExecRes::Store { addr, kind } => match kind {
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
                ExecRes::Mfc0 { from, .. } => {
                    read = cop0.regs[from];
                }
                ExecRes::Mtc0 { dest, res } => {
                    cop0.regs[dest] = res;
                }
                _ => {}
            }

            *stage = Latch::Memory {
                pc,
                opcode,
                exec,
                read,
            };
        }

        Ok(())
    }

    /// Write to register file.
    pub fn writeback(&mut self, regs: &mut Registers) {
        if let stage @ &mut Latch::Memory {
            pc,
            opcode,
            exec,
            read,
        } = &mut self.queue[4]
        {
            match exec {
                ExecRes::Alu {
                    dest,
                    res: Some(res),
                } => {
                    // No overflow, because checked already
                    regs.general[dest] = res;
                }
                ExecRes::MulDiv { hi, lo } => {
                    regs.hi = hi;
                    regs.lo = lo;
                }
                ExecRes::Branch { link: true, .. } => {
                    regs.general[31] = pc + 8;
                }
                ExecRes::Jump {
                    link: true,
                    link_reg,
                    ..
                } => {
                    regs.general[link_reg] = pc + 8;
                }
                ExecRes::Load { dest, .. } | ExecRes::Mfc0 { dest, .. } => {
                    regs.general[dest] = read;
                }
                _ => {}
            }

            // Always zero
            regs.general[0] = 0;

            *stage = Latch::WrittenBack { opcode, exec };
        }
    }

    /// Flush the pipeline with [`count`] of instructions.
    /// If the next stage is branch or jump, flush it too.
    /// Returns true if the last flushed op is in branch delay slot.
    pub fn flush(&mut self, count: usize) -> bool {
        for i in 0..count {
            self.queue[i] = Latch::Flushed;
        }

        // Flush branch/jump operation older than failed delay slot and save its PC
        let mut parent = false;
        if let (1, stage @ &mut Latch::Decoded { opcode, .. })
        | (2, stage @ &mut Latch::Executed { opcode, .. })
        | (3, stage @ &mut Latch::Memory { opcode, .. })
        | (4, stage @ &mut Latch::WrittenBack { opcode, .. }) = (count, &mut self.queue[count])
            && opcode.branch_delay()
        {
            parent = true;
            *stage = Latch::Flushed;
        }

        parent
    }
}
