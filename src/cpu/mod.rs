use arraydeque::{ArrayDeque, Wrapping};

mod ins;

pub struct Bus {
    // TODO
    pub memory: Vec<u8>,
}

impl Bus {
    pub fn read_byte(&self, addr: u32) -> u8 {
        self.memory[addr as usize]
    }

    pub fn read_half(&self, addr: u32) -> u16 {
        u16::from_le_bytes([self.memory[addr as usize], self.memory[(addr + 1) as usize]])
    }

    pub fn read_word(&self, addr: u32) -> u32 {
        // TODO : check alignment
        println!("{:?}", &self.memory[addr as usize..][..4]);
        u32::from_le_bytes([
            self.memory[addr as usize],
            self.memory[(addr + 1) as usize],
            self.memory[(addr + 2) as usize],
            self.memory[(addr + 3) as usize],
        ])
    }

    pub fn store_byte(&mut self, addr: u32, value: u8) {
        todo!()
    }

    pub fn store_half(&mut self, addr: u32, value: u16) {
        todo!()
    }

    pub fn store_word(&mut self, addr: u32, value: u32) {
        let [a, b, c, d] = value.to_le_bytes();
        self.memory[addr as usize] = a;
        self.memory[(addr + 1) as usize] = b;
        self.memory[(addr + 2) as usize] = c;
        self.memory[(addr + 3) as usize] = d;
    }
}

#[derive(Debug)]
pub struct Cpu {
    pub regs: Registers,
    pub pipeline: Pipeline,
}

#[derive(Default, Debug)]
pub struct Registers {
    pub general: [u32; 32],
    // TODO : it's not 0x0, rather 0xBFC00000
    pub pc: u32,
    pub hi: u32,
    pub lo: u32,
}

#[derive(Default, Debug)]
pub struct Pipeline {
    queue: ArrayDeque<PipelineStage, 5, Wrapping>,
}

#[derive(Debug)]
pub enum PipelineStage {
    Flushed,
    Fetched(u32),
    Decoded(ins::OpResult),
    Executed {
        op: ins::OpResult,
        link_addr: u32,
    },
    Memory {
        op: ins::OpResult,
        link_addr: u32,
        read: u32,
    },
}

impl Cpu {
    pub fn cycle(&mut self, bus: &mut Bus) {
        self.pipeline.fetch(&mut self.regs, bus);
        self.pipeline.decode(&mut self.regs);
        self.pipeline.execute(&mut self.regs);
        self.pipeline.memory(bus);
        self.pipeline.writeback(&mut self.regs);
    }
}

impl Pipeline {
    pub fn fetch(&mut self, regs: &mut Registers, bus: &Bus) {
        let ins = bus.read_word(regs.pc);
        self.queue.push_front(PipelineStage::Fetched(ins));
        regs.pc = regs.pc.wrapping_add(4);
    }

    // TODO : fallible
    pub fn decode(&mut self, regs: &mut Registers) {
        if let Some(stage @ &mut PipelineStage::Fetched(ins)) = self.queue.get_mut(1) {
            // TODO : don't set nop, instead bail an error or trap
            let op = ins::OpResult::decode_and_evaluate(ins, regs).unwrap_or_default();
            match op {
                ins::OpResult::Alu { dest, res } => {
                    // TODO : overflow trap
                    regs.general[dest] = res.unwrap();
                }
                ins::OpResult::MulDiv {
                    res: Some((hi, lo)),
                } => {
                    regs.hi = hi;
                    regs.lo = lo;
                }
                _ => (),
            }
            *stage = PipelineStage::Decoded(op);
        }
    }

    pub fn execute(&mut self, regs: &mut Registers) {
        // To overcome borrow checker
        // Don't really want to split array into mutable parts
        let mut jump_addr = None;
        if let Some(stage @ &mut PipelineStage::Decoded(op)) = self.queue.get_mut(2) {
            match op {
                ins::OpResult::Jump { addr, .. } => {
                    jump_addr = Some(addr);
                }
                ins::OpResult::Branch { addr, .. } => {
                    jump_addr = addr;
                }
                _ => {}
            }
            *stage = PipelineStage::Executed {
                op,
                link_addr: regs.pc - 4,
            };
        }

        if let Some(addr) = jump_addr {
            // Flush pre-fetched, but keep decoded (so called delay slot)
            self.queue[0] = PipelineStage::Flushed;
            regs.pc = addr;
        }
    }

    pub fn memory(&mut self, bus: &mut Bus) {
        if let Some(stage @ &mut PipelineStage::Executed { op, link_addr }) = self.queue.get_mut(3)
        {
            let mut read = 0;
            match op {
                ins::OpResult::Load { addr, kind, .. } => {
                    read = match kind {
                        ins::LoadKind::IByte => bus.read_byte(addr).cast_signed() as u32,
                        ins::LoadKind::IHalf => bus.read_half(addr).cast_signed() as u32,
                        ins::LoadKind::UByte => bus.read_byte(addr) as u32,
                        ins::LoadKind::UHalf => bus.read_half(addr) as u32,
                        ins::LoadKind::Word => bus.read_word(addr),
                        ins::LoadKind::WordLeft => todo!(),
                        ins::LoadKind::WordRight => todo!(),
                    };
                }
                ins::OpResult::Store { addr, kind } => match kind {
                    ins::StoreKind::Byte(val) => {
                        bus.store_byte(addr, val);
                    }
                    ins::StoreKind::Half(val) => {
                        bus.store_half(addr, val);
                    }
                    ins::StoreKind::Word(val) => {
                        bus.store_word(addr, val);
                    }
                    ins::StoreKind::WordLeft(val) => todo!(),
                    ins::StoreKind::WordRight(val) => todo!(),
                },
                _ => {}
            }

            *stage = PipelineStage::Memory {
                op,
                link_addr,
                read,
            };
        }
    }

    pub fn writeback(&mut self, regs: &mut Registers) {
        if let Some(&mut PipelineStage::Memory {
            op,
            link_addr,
            read,
        }) = self.queue.get_mut(4)
        {
            match op {
                ins::OpResult::Load { dest, .. } => {
                    regs.general[dest] = read;
                }
                ins::OpResult::Branch { link: true, .. } => {
                    regs.general[31] = link_addr;
                }
                ins::OpResult::Jump {
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
