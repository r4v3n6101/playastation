use arraydeque::{ArrayDeque, Wrapping};

use crate::interconnect::{Bus, BusError, BusErrorKind};

use super::{Cpu, Exception, Registers, cop0::Cop0, ins::Opcode};

mod ops;

#[derive(Debug)]
struct Error {
    pc: u32,
    kind: ErrorKind,
}

#[derive(Debug)]
enum ErrorKind {
    AluOverflow,
    InvalidInstruction(u32),
    InsLoad(BusError),
    MemoryLoad(BusError),
    MemoryStore(BusError),
    Break,
    Syscall,
    Interrupt,
}

#[derive(Debug)]
pub struct State {
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
        op: Opcode,
        regs: Registers,
    },
    Executed {
        pc: u32,
        op: Opcode,
        exec: ops::ExecRes,
    },
    Memory {
        pc: u32,
        op: Opcode,
        exec: ops::ExecRes,
        read: u32,
    },
    WrittenBack {
        op: Opcode,
        exec: ops::ExecRes,
        read: u32,
    },
}

impl Default for State {
    fn default() -> Self {
        Self {
            queue: ArrayDeque::from([Latch::Flushed; 6]),
        }
    }
}

impl State {
    pub fn run(&mut self, cpu: &mut Cpu, bus: &mut Bus) -> Result<(), (bool, u32, Exception)> {
        let fetch = self.fetch(&mut cpu.regs.pc, &cpu.cop0, bus);
        let decode = self.decode(&cpu.regs);
        let execute = self.execute(&mut cpu.regs.pc, &mut cpu.cop0);
        let mem = self.memory(bus, &mut cpu.cop0);
        self.writeback(&mut cpu.regs);

        let (err, flush_count) = if let Err(err) = mem {
            (err, 4)
        } else if let Err(err) = execute {
            (err, 3)
        } else if let Err(err) = decode {
            (err, 2)
        } else if let Err(err) = fetch {
            (err, 1)
        } else {
            return Ok(());
        };

        let has_delay_slot = self.flush(flush_count);
        let exception = match err.kind {
            ErrorKind::InvalidInstruction(_) => Exception::ReservedInstruction,
            ErrorKind::AluOverflow => Exception::Overflow,
            ErrorKind::Break => Exception::Break,
            ErrorKind::Syscall => Exception::Syscall,
            ErrorKind::Interrupt => Exception::Interrupt,

            ErrorKind::MemoryLoad(BusError {
                bad_vaddr,
                kind: BusErrorKind::UnalignedAddr,
            })
            | ErrorKind::InsLoad(BusError {
                bad_vaddr,
                kind: BusErrorKind::UnalignedAddr,
            }) => Exception::UnalignedLoad { bad_vaddr },

            ErrorKind::MemoryStore(BusError {
                bad_vaddr,
                kind: BusErrorKind::UnalignedAddr,
            }) => Exception::UnalignedStore { bad_vaddr },

            ErrorKind::InsLoad(BusError { bad_vaddr, .. }) => {
                Exception::InstructionBus { bad_vaddr }
            }
            ErrorKind::MemoryLoad(BusError { bad_vaddr, .. })
            | ErrorKind::MemoryStore(BusError { bad_vaddr, .. }) => {
                Exception::DataBus { bad_vaddr }
            }
        };

        Err((has_delay_slot, err.pc, exception))
    }

    /// Fetch. Read an instruction and increment PC (program count).
    /// May be interrupted from the outside.
    fn fetch(&mut self, pc: &mut u32, cop0: &Cop0, bus: &Bus) -> Result<(), Error> {
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
            ins: u32::from_le_bytes(bus.load(*pc).map_err(|err| Error {
                pc: *pc,
                kind: ErrorKind::InsLoad(err),
            })?),
        });
        *pc = pc.wrapping_add(4);

        Ok(())
    }

    /// Decode operation and save snapshot of registers.
    fn decode(&mut self, regs: &Registers) -> Result<(), Error> {
        if let stage @ &mut Latch::Fetched { pc, ins } = &mut self.queue[1] {
            let op = Opcode::decode(ins).ok_or(Error {
                pc,
                kind: ErrorKind::InvalidInstruction(ins),
            })?;

            *stage = Latch::Decoded {
                pc,
                ins,
                op,
                regs: *regs,
            };
        }

        Ok(())
    }

    /// Execute jumps and branches.
    /// Used instead of emulating branch delay slot.
    fn execute(&mut self, next_pc: &mut u32, cop0: &mut Cop0) -> Result<(), Error> {
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
            op,
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
                        ops::ExecRes::Alu {
                            dest,
                            res: Some(res),
                        } => {
                            regs.general[dest] = res;
                        }
                        ops::ExecRes::MulDiv { hi, lo } => {
                            regs.hi = hi;
                            regs.lo = lo;
                        }
                        // TODO : this must not be forwarded, but I don't know real semantic of
                        // this operation
                        ops::ExecRes::Mfc0 { dest, from } => {
                            regs.general[dest] = cop0.regs[from];
                        }
                        ops::ExecRes::Load { dest, .. } => {
                            if let Some(
                                Latch::Memory { read, .. } | Latch::WrittenBack { read, .. },
                            ) = latch
                            {
                                regs.general[dest] = read;
                            }
                        }
                        _ => {}
                    }
                }

                // Always
                regs.general[0] = 0;
            }

            let exec = ops::execute(ins, op, &regs);
            match exec {
                ops::ExecRes::Jump { addr, .. } => {
                    jump_addr = Some(addr);
                }
                ops::ExecRes::Branch { addr, .. } => {
                    jump_addr = addr;
                }
                ops::ExecRes::Alu { res: None, .. } => {
                    return Err(Error {
                        pc,
                        kind: ErrorKind::AluOverflow,
                    });
                }
                ops::ExecRes::Break => {
                    return Err(Error {
                        pc,
                        kind: ErrorKind::Break,
                    });
                }
                ops::ExecRes::Syscall => {
                    return Err(Error {
                        pc,
                        kind: ErrorKind::Syscall,
                    });
                }
                ops::ExecRes::Rfe => {
                    cop0.exception_leave();
                }
                _ => {}
            }

            *stage = Latch::Executed { pc, op, exec };
        }

        if let Some(addr) = jump_addr {
            // Flush pre-fetched, but keep decoded (so called delay slot)
            self.queue[0] = Latch::Flushed;
            *next_pc = addr;
        }

        Ok(())
    }

    /// Operations with memory like load/store. Eliminate need for load-delay slot.
    fn memory(&mut self, bus: &mut Bus, cop0: &mut Cop0) -> Result<(), Error> {
        if let stage @ &mut Latch::Executed { pc, op, exec } = &mut self.queue[3] {
            let mut read = 0;
            match exec {
                ops::ExecRes::Load { addr, kind, .. } => {
                    read = match kind {
                        ops::LoadKind::IByte => {
                            i8::from_le_bytes(bus.load(addr).map_err(|err| Error {
                                pc,
                                kind: ErrorKind::MemoryLoad(err),
                            })?) as u32
                        }
                        ops::LoadKind::IHalf => {
                            i16::from_le_bytes(bus.load(addr).map_err(|err| Error {
                                pc,
                                kind: ErrorKind::MemoryLoad(err),
                            })?) as u32
                        }
                        ops::LoadKind::UByte => {
                            u8::from_le_bytes(bus.load(addr).map_err(|err| Error {
                                pc,
                                kind: ErrorKind::MemoryLoad(err),
                            })?)
                            .into()
                        }
                        ops::LoadKind::UHalf => {
                            u16::from_le_bytes(bus.load(addr).map_err(|err| Error {
                                pc,
                                kind: ErrorKind::MemoryLoad(err),
                            })?)
                            .into()
                        }
                        ops::LoadKind::Word => {
                            u32::from_le_bytes(bus.load(addr).map_err(|err| Error {
                                pc,
                                kind: ErrorKind::MemoryLoad(err),
                            })?)
                        }
                        ops::LoadKind::WordLeft => todo!(),
                        ops::LoadKind::WordRight => todo!(),
                    };
                }
                ops::ExecRes::Store { addr, kind } if !cop0.status().isc() => match kind {
                    ops::StoreKind::Byte(val) => {
                        bus.store(addr, val.to_le_bytes()).map_err(|err| Error {
                            pc,
                            kind: ErrorKind::MemoryStore(err),
                        })?;
                    }
                    ops::StoreKind::Half(val) => {
                        bus.store(addr, val.to_le_bytes()).map_err(|err| Error {
                            pc,
                            kind: ErrorKind::MemoryStore(err),
                        })?;
                    }
                    ops::StoreKind::Word(val) => {
                        bus.store(addr, val.to_le_bytes()).map_err(|err| Error {
                            pc,
                            kind: ErrorKind::MemoryStore(err),
                        })?;
                    }
                    ops::StoreKind::WordLeft(_) => todo!(),
                    ops::StoreKind::WordRight(_) => todo!(),
                },
                ops::ExecRes::Mfc0 { from, .. } => {
                    read = cop0.regs[from];
                }
                ops::ExecRes::Mtc0 { dest, res } => {
                    cop0.regs[dest] = res;
                }
                _ => {}
            }

            *stage = Latch::Memory { pc, op, exec, read };
        }

        Ok(())
    }

    /// Write to register file.
    fn writeback(&mut self, regs: &mut Registers) {
        if let stage @ &mut Latch::Memory { pc, op, exec, read } = &mut self.queue[4] {
            match exec {
                ops::ExecRes::Alu {
                    dest,
                    res: Some(res),
                } => {
                    // No overflow, because checked already
                    regs.general[dest] = res;
                }
                ops::ExecRes::MulDiv { hi, lo } => {
                    regs.hi = hi;
                    regs.lo = lo;
                }
                ops::ExecRes::Branch { link: true, .. } => {
                    regs.general[31] = pc + 8;
                }
                ops::ExecRes::Jump {
                    link: true,
                    link_reg,
                    ..
                } => {
                    regs.general[link_reg] = pc + 8;
                }
                ops::ExecRes::Load { dest, .. } | ops::ExecRes::Mfc0 { dest, .. } => {
                    regs.general[dest] = read;
                }
                _ => {}
            }

            // Always zero
            regs.general[0] = 0;

            *stage = Latch::WrittenBack { op, exec, read };
        }
    }

    /// Flush the pipeline with [`count`] of instructions.
    /// If the next stage is branch or jump, flush it too.
    /// Returns true if the last flushed op is in branch delay slot.
    fn flush(&mut self, count: usize) -> bool {
        for i in 0..count {
            self.queue[i] = Latch::Flushed;
        }

        // Flush branch/jump operation older than failed delay slot and save its PC
        let mut parent = false;
        if let (1, stage @ &mut Latch::Decoded { op, .. })
        | (2, stage @ &mut Latch::Executed { op, .. })
        | (3, stage @ &mut Latch::Memory { op, .. })
        | (4, stage @ &mut Latch::WrittenBack { op, .. }) = (count, &mut self.queue[count])
            && op.has_branch_delay()
        {
            parent = true;
            *stage = Latch::Flushed;
        }

        parent
    }
}
