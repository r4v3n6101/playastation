use alloc::{boxed::Box, rc::Rc, vec::Vec};
use core::mem;

use fnv::FnvBuildHasher;
use hashbrown::HashMap;
use smallvec::SmallVec;

use crate::{
    RAM_SIZE,
    cpu::{Cpu, TranslationResult},
    interconnect::{Region, region_of},
};

const PAGE_BITS: usize = 12;
const PAGE_SIZE: usize = 1 << PAGE_BITS;
const PAGES: usize = RAM_SIZE / PAGE_SIZE;

pub trait CacheEntry {
    fn num_of_ops(&self) -> usize;
}

pub struct PagedCache<T> {
    /// `phys_pc` -> [`Cached`]
    by_pc: HashMap<u32, Rc<Cached<T>>, FnvBuildHasher>,
    /// Page -> array of [`Cached`]
    pages: Box<[Vec<u32>; PAGES]>,
}

pub struct Cached<T> {
    pub entry: T,
    pages: SmallVec<[usize; 2]>,
}

impl<T> Default for PagedCache<T> {
    fn default() -> Self {
        Self {
            by_pc: HashMap::default(),
            pages: Box::new([const { Vec::new() }; _]),
        }
    }
}

impl<T> PagedCache<T> {
    pub fn invalidate_all(&mut self) {
        self.pages.iter_mut().for_each(Vec::clear);
        self.by_pc.clear();
    }

    /// Returns `true` if entry is invalidated
    pub fn invalidate(&mut self, paddr: u32, cached: Option<&Cached<T>>) -> bool {
        if let Region::Ram = region_of(paddr) {
            let page = page_of(paddr);
            for phys_pc in self.pages[page].drain(..) {
                self.by_pc.remove(&phys_pc);
            }

            if let Some(cached) = cached
                && cached.pages.contains(&page)
            {
                return true;
            }
        }

        false
    }

    pub fn get(&self, cpu: &Cpu) -> Option<Rc<Cached<T>>> {
        let TranslationResult::PhysAddr(phys_pc) = cpu.mmu.translate_addr(cpu.pc) else {
            unimplemented!()
        };

        let phys_pc = maybe_cut_ram_mirrors(phys_pc);
        self.by_pc.get(&phys_pc).map(Rc::clone)
    }
}

impl<T> PagedCache<T>
where
    T: CacheEntry,
{
    pub fn insert(&mut self, phys_pc: u32, entry: T) -> Rc<Cached<T>> {
        let phys_pc = maybe_cut_ram_mirrors(phys_pc);

        let pages = pages_for_block(phys_pc, entry.num_of_ops());
        let block = Rc::new(Cached { entry, pages });

        self.by_pc.insert(phys_pc, Rc::clone(&block));

        for page in &block.pages {
            self.pages[*page].push(phys_pc);
        }

        block
    }
}

fn pages_for_block(phys_pc: u32, num_ops: usize) -> SmallVec<[usize; 2]> {
    let mut pages = SmallVec::new();

    if num_ops != 0 && region_of(phys_pc) == Region::Ram {
        let end_exclusive = phys_pc.saturating_add((num_ops * mem::size_of::<u32>()) as u32);
        let tracked_end_exclusive = end_exclusive.min(RAM_SIZE as u32);

        for page in page_of(phys_pc)..=page_of(tracked_end_exclusive - 1) {
            pages.push(page);
        }
    }

    pages
}

fn page_of(paddr: u32) -> usize {
    ((paddr as usize) & (RAM_SIZE - 1)) >> PAGE_BITS
}

fn maybe_cut_ram_mirrors(paddr: u32) -> u32 {
    if let Region::Ram = region_of(paddr) {
        paddr & (RAM_SIZE as u32 - 1)
    } else {
        paddr
    }
}
