use alloc::{boxed::Box, rc::Rc, vec::Vec};
use core::mem;

use fnv::FnvBuildHasher;
use hashbrown::HashMap;
use slotmap::{SlotMap, new_key_type};
use smallvec::SmallVec;

use crate::{
    cpu::{Cpu, Exception, Opcode, TranslationResult::PhysAddr},
    globals::RAM_SIZE,
    interconnect::{Bus, Region, region_of},
};

mod decoder;

const PAGE_BITS: usize = 12;
const PAGE_SIZE: usize = 1 << PAGE_BITS;
const PAGES: usize = RAM_SIZE / PAGE_SIZE;
const OPS_PER_BLOCK: usize = PAGE_SIZE / mem::size_of::<u32>();

new_key_type! {
    struct BlockKey;
}

#[derive(Debug)]
pub struct PagedCache {
    /// [`Block`]s storage.
    blocks: SlotMap<BlockKey, Rc<Block>>,
    /// Page -> array of [`Block`]
    pages: Box<[Vec<BlockKey>; PAGES]>,
    /// `phys_pc` -> [`Block`]
    by_pc: HashMap<u32, BlockKey, FnvBuildHasher>,
}

#[derive(Debug)]
pub struct Block {
    pub ops: Vec<Operation>,
    pub pages: SmallVec<[usize; 2]>,
    phys_pc: u32,
}

#[derive(Debug)]
pub enum Operation {
    Instruction { pc: u32, ins: u32, op: Opcode },
    Error { pc: u32, cause: Exception },
}

impl Default for PagedCache {
    fn default() -> Self {
        Self {
            blocks: SlotMap::default(),
            pages: Box::new([const { Vec::new() }; _]),
            by_pc: HashMap::default(),
        }
    }
}

impl PagedCache {
    pub fn get_or_fetch_decode_block(&mut self, cpu: &mut Cpu, bus: &mut Bus) -> Rc<Block> {
        let PhysAddr(phys_pc) = cpu.mmu.translate_addr(cpu.pc) else {
            unimplemented!()
        };

        self.by_pc
            .get(&phys_pc)
            .and_then(|&block_key| self.blocks.get(block_key))
            .map(Rc::clone)
            .unwrap_or_else(|| {
                self.insert_block(
                    phys_pc,
                    decoder::fetch_and_decode_block(OPS_PER_BLOCK, cpu, bus),
                    matches!(region_of(phys_pc), Region::Ram),
                )
            })
    }

    pub fn invalidate_page(&mut self, paddr: u32) -> Option<usize> {
        let mut invalidated = None;
        if let Region::Ram = region_of(paddr) {
            let page = page_of(paddr);
            for block_key in self.pages[page].drain(..) {
                if let Some(block) = self.blocks.remove(block_key) {
                    self.by_pc.remove(&block.phys_pc);
                }
            }

            invalidated = Some(page);
        }

        invalidated
    }

    fn insert_block(
        &mut self,
        phys_pc: u32,
        ops: Vec<Operation>,
        page_tracking: bool,
    ) -> Rc<Block> {
        let end_pc = phys_pc
            .wrapping_add((ops.len() * mem::size_of::<u32>()) as u32)
            .wrapping_sub(1);
        let mut block = Block {
            phys_pc,
            ops,
            pages: SmallVec::new(),
        };
        if page_tracking {
            for page in page_of(phys_pc)..=page_of(end_pc) {
                block.pages.push(page);
            }
        }

        let block = Rc::new(block);
        let block_key = self.blocks.insert(Rc::clone(&block));
        self.by_pc.insert(phys_pc, block_key);

        if page_tracking {
            for page in page_of(phys_pc)..=page_of(end_pc) {
                self.pages[page].push(block_key);
            }
        }

        block
    }
}

fn page_of(paddr: u32) -> usize {
    debug_assert_eq!(region_of(paddr), Region::Ram);

    ((paddr as usize) % RAM_SIZE) >> PAGE_BITS
}
