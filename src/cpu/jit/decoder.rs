use crate::interconnect::{Bus, BusError, BusErrorKind};

use super::super::{Exception, ins::Opcode};

pub enum DecRes {
    Decoded {
        pc: u32,
        ins: u32,
        in_delay_slot: bool,
        op: Opcode,
    },
    Exception {
        pc: u32,
        in_delay_slot: bool,
        exc: Exception,
    },
}

pub struct InsIter<'a> {
    pc: &'a mut u32,
    bus: &'a Bus,
    left: usize,
    pending_delay_slot: bool,
}

impl<'a> InsIter<'a> {
    pub fn new_start_from(pc: &'a mut u32, bus: &'a Bus, size: usize) -> Self {
        Self {
            pc,
            bus,
            left: size,
            pending_delay_slot: false,
        }
    }

    fn pend_delay_slot(&mut self) {
        self.pending_delay_slot = true;
    }

    fn enough(&mut self) {
        // Cancel pending instruction
        self.pending_delay_slot = false;
        // Limit is exceeded
        self.left = 0;
    }
}

impl Iterator for InsIter<'_> {
    type Item = DecRes;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pending_delay_slot {
            // Ignore skip when left=0
            self.left = self.left.wrapping_sub(1);
        } else {
            self.left = self.left.checked_sub(1)?;
        }

        let pc = *self.pc;
        let in_delay_slot = self.pending_delay_slot;
        let ins = match self.bus.read_word(pc) {
            Ok(ins) => ins,
            Err(BusError { bad_vaddr, kind }) => {
                self.enough();
                return Some(DecRes::Exception {
                    pc,
                    in_delay_slot,
                    exc: match kind {
                        BusErrorKind::UnalignedAddr => Exception::UnalignedLoad { bad_vaddr },
                        BusErrorKind::Unmapped => Exception::InstructionBus { bad_vaddr },
                    },
                });
            }
        };
        let Some(op) = Opcode::decode(ins) else {
            self.enough();
            return Some(DecRes::Exception {
                pc,
                in_delay_slot,
                exc: Exception::ReservedInstruction,
            });
        };

        if let Opcode::Syscall = op {
            self.enough();
            return Some(DecRes::Exception {
                pc,
                in_delay_slot,
                exc: Exception::Syscall,
            });
        } else if let Opcode::Break = op {
            self.enough();
            return Some(DecRes::Exception {
                pc,
                in_delay_slot,
                exc: Exception::Break,
            });
        } else if self.pending_delay_slot {
            self.enough();
        } else if op.has_branch_delay() {
            self.pend_delay_slot();
        }

        *self.pc = pc.wrapping_add(4);

        Some(DecRes::Decoded {
            pc,
            ins,
            in_delay_slot,
            op,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bus(words: &[(u32, u32)]) -> Bus {
        let mut bus = Bus::default();

        words.iter().for_each(|&(addr, val)| {
            let _ = bus.store_word(addr, val);
        });

        bus
    }

    #[test]
    fn stops_at_branch_after_delay_slot() {
        // beq $zero, $zero, +1
        // nop
        // nop   <- must NOT be included
        let bus = make_bus(&[
            (0x0000_0000, 0x1000_0001),
            (0x0000_0004, 0x0000_0000),
            (0x0000_0008, 0x0000_0000),
        ]);

        let mut pc = 0x0000_0000;
        let out = InsIter::new_start_from(&mut pc, &bus, 16).collect::<Vec<_>>();

        assert_eq!(out.len(), 2);

        match &out[0] {
            DecRes::Decoded { pc, ins, op, .. } => {
                assert_eq!(*pc, 0x0000_0000);
                assert_eq!(*ins, 0x1000_0001);
                assert!(op.has_branch_delay());
            }
            _ => panic!("expected decoded branch"),
        }

        match &out[1] {
            DecRes::Decoded {
                pc,
                ins,
                in_delay_slot,
                ..
            } => {
                assert_eq!(*pc, 0x0000_0004);
                assert_eq!(*ins, 0x0000_0000);
                assert!(in_delay_slot);
            }
            _ => panic!("expected decoded delay slot"),
        }

        assert_eq!(pc, 0x0000_0008);
    }

    #[test]
    fn stops_at_jr_after_delay_slot() {
        // jr $ra
        // nop
        // nop   <- must NOT be included
        let bus = make_bus(&[
            (0x0000_0000, 0x03E0_0008),
            (0x0000_0004, 0x0000_0000),
            (0x0000_0008, 0x0000_0000),
        ]);

        let mut pc = 0x0000_0000;
        let out = InsIter::new_start_from(&mut pc, &bus, 16).collect::<Vec<_>>();

        assert_eq!(out.len(), 2);

        match &out[0] {
            DecRes::Decoded { pc, op, .. } => {
                assert_eq!(*pc, 0);
                assert!(op.has_branch_delay());
            }
            _ => panic!("expected decoded jr"),
        }

        match &out[1] {
            DecRes::Decoded {
                pc, in_delay_slot, ..
            } => {
                assert_eq!(*pc, 4);
                assert!(in_delay_slot);
            }
            _ => panic!("expected decoded delay slot"),
        }

        assert_eq!(pc, 8);
    }

    #[test]
    fn stops_immediately_on_syscall() {
        // syscall
        // nop   <- must NOT be included
        let bus = make_bus(&[(0x0000_0000, 0x0000_000C), (0x0000_0004, 0x0000_0000)]);

        let mut pc = 0x0000_0000;
        let out = InsIter::new_start_from(&mut pc, &bus, 16).collect::<Vec<_>>();

        assert_eq!(out.len(), 1);

        match &out[0] {
            DecRes::Exception {
                pc,
                exc: Exception::Syscall,
                ..
            } => {
                assert_eq!(*pc, 0);
            }
            _ => panic!("expected decoded syscall"),
        }
    }

    #[test]
    fn stops_immediately_on_break() {
        // break
        // nop   <- must NOT be included
        let bus = make_bus(&[(0x0000_0000, 0x0000_000D), (0x0000_0004, 0x0000_0000)]);

        let mut pc = 0x0000_0000;
        let out = InsIter::new_start_from(&mut pc, &bus, 16).collect::<Vec<_>>();

        assert_eq!(out.len(), 1);

        match &out[0] {
            DecRes::Exception {
                pc,
                exc: Exception::Break,
                ..
            } => {
                assert_eq!(*pc, 0);
            }
            _ => panic!("expected decoded break"),
        }
    }

    #[test]
    fn returns_reserved_instruction_exception_and_stops() {
        // 0xFFFF_FFFF should not decode on MIPS
        let bus = make_bus(&[(0x0000_0000, 0xFFFF_FFFF), (0x0000_0004, 0x0000_0000)]);

        let mut pc = 0x0000_0000;
        let out = InsIter::new_start_from(&mut pc, &bus, 16).collect::<Vec<_>>();

        assert_eq!(out.len(), 1);
        assert!(matches!(
            out[0],
            DecRes::Exception {
                exc: Exception::ReservedInstruction,
                ..
            }
        ));

        // PC should stay on faulting instruction fetch point in this implementation.
        assert_eq!(pc, 0);
    }

    #[test]
    fn respects_size_limit_when_no_terminator() {
        let bus = make_bus(&[
            (0x0000_0000, 0x0000_0000),
            (0x0000_0004, 0x0000_0000),
            (0x0000_0008, 0x0000_0000),
        ]);

        let mut pc = 0x0000_0000;
        let out = InsIter::new_start_from(&mut pc, &bus, 2).collect::<Vec<_>>();

        assert_eq!(out.len(), 2);

        match &out[0] {
            DecRes::Decoded { pc, .. } => assert_eq!(*pc, 0),
            _ => panic!("expected decoded ins 0"),
        }
        match &out[1] {
            DecRes::Decoded { pc, .. } => assert_eq!(*pc, 4),
            _ => panic!("expected decoded ins 1"),
        }

        assert_eq!(pc, 8);
    }
}
