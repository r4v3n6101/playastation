use alloc::{boxed::Box, rc::Rc, vec::Vec};
use core::mem;

use fnv::FnvBuildHasher;
use hashbrown::HashMap;
use smallvec::SmallVec;

use crate::{
    cpu::{Cpu, TranslationResult},
    interconnect::{RAM_SIZE, Region, region_of},
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
            while let Some(phys_pc) = self.pages[page].pop() {
                if let Some(block) = self.by_pc.remove(&phys_pc) {
                    let other_pages = block.pages.iter().copied().filter(|&p| p != page);
                    self.unlink_pages(phys_pc, other_pages);
                }
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

    fn unlink_pages(&mut self, phys_pc: u32, pages: impl IntoIterator<Item = usize>) {
        for page in pages {
            let entries = &mut self.pages[page];
            if let Some(index) = entries.iter().position(|&pc| pc == phys_pc) {
                entries.swap_remove(index);
            }
        }
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

        if let Some(old) = self.by_pc.insert(phys_pc, Rc::clone(&block)) {
            self.unlink_pages(phys_pc, old.pages.iter().copied());
        }

        for page in &block.pages {
            self.pages[*page].push(phys_pc);
        }

        block
    }
}

fn pages_for_block(phys_pc: u32, num_ops: usize) -> SmallVec<[usize; 2]> {
    let mut pages = SmallVec::new();

    if num_ops != 0 && region_of(phys_pc) == Region::Ram {
        let first_page = page_of(phys_pc);
        let byte_len = num_ops.saturating_mul(mem::size_of::<u32>());
        let page_offset = phys_pc as usize & (PAGE_SIZE - 1);
        let page_count = page_offset
            .saturating_add(byte_len)
            .div_ceil(PAGE_SIZE)
            .min(PAGES);

        for offset in 0..page_count {
            // A block can continue into the next mirror at the end of RAM.
            pages.push((first_page + offset) % PAGES);
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

#[cfg(test)]
mod tests {
    use super::*;

    struct Block(usize);

    impl CacheEntry for Block {
        fn num_of_ops(&self) -> usize {
            self.0
        }
    }

    fn cpu_at(pc: u32) -> Cpu {
        Cpu {
            pc,
            ..Cpu::default()
        }
    }

    #[test]
    fn invalidation_unlinks_a_block_from_all_its_pages() {
        let mut cache = PagedCache::default();
        let pc = PAGE_SIZE as u32 - 4;
        let block = cache.insert(pc, Block(2));
        let neighbor_pc = PAGE_SIZE as u32 + 0x100;
        let neighbor = cache.insert(neighbor_pc, Block(1));

        assert!(cache.invalidate(0, Some(&block)));
        assert!(cache.get(&cpu_at(pc)).is_none());
        assert!(cache.pages[0].is_empty());
        assert_eq!(cache.pages[1].as_slice(), &[neighbor_pc]);
        assert!(Rc::ptr_eq(
            &cache.get(&cpu_at(neighbor_pc)).unwrap(),
            &neighbor
        ));
    }

    #[test]
    fn replacing_a_block_removes_its_old_page_links() {
        let pc = PAGE_SIZE as u32 - 4;
        let mut cache = PagedCache::default();
        cache.insert(pc, Block(2));

        // The new block ends before the next page, unlike the old one.
        let replacement = cache.insert(pc, Block(1));
        assert!(!cache.invalidate(PAGE_SIZE as u32, Some(&replacement)));
        assert!(Rc::ptr_eq(&cache.get(&cpu_at(pc)).unwrap(), &replacement));
        assert_eq!(cache.pages[0].as_slice(), &[pc]);
        assert!(cache.pages[1].is_empty());
    }

    #[test]
    fn reinserting_a_block_does_not_keep_invalidated_page_links() {
        let pc = PAGE_SIZE as u32 - 4;
        let mut cache = PagedCache::default();
        cache.insert(pc, Block(2));
        cache.invalidate(0, None);

        let replacement = cache.insert(pc, Block(1));
        assert!(!cache.invalidate(PAGE_SIZE as u32, Some(&replacement)));
        assert!(Rc::ptr_eq(&cache.get(&cpu_at(pc)).unwrap(), &replacement));
        assert_eq!(cache.pages[0].as_slice(), &[pc]);
        assert!(cache.pages[1].is_empty());
    }

    #[test]
    fn writes_to_ram_mirrors_invalidate_a_wrapped_block() {
        let ram_size = RAM_SIZE as u32;
        for mirror in 0..4 {
            let mut cache = PagedCache::default();
            // The second instruction is at the beginning of the next RAM mirror.
            let pc = 2 * ram_size - 4;
            let block = cache.insert(pc, Block(2));

            assert!(cache.invalidate(mirror * ram_size, Some(&block)));
            assert!(cache.get(&cpu_at(0x8000_0000 | pc)).is_none());
            assert!(cache.pages[PAGES - 1].is_empty());
            assert!(cache.pages[0].is_empty());
        }
    }

    #[test]
    fn a_block_ending_at_a_page_boundary_does_not_occupy_the_next_page() {
        let pc = PAGE_SIZE as u32 - 8;
        let mut cache = PagedCache::default();
        let block = cache.insert(pc, Block(2));

        assert!(!cache.invalidate(PAGE_SIZE as u32, Some(&block)));
        assert!(Rc::ptr_eq(&cache.get(&cpu_at(pc)).unwrap(), &block));
    }

    #[test]
    fn a_block_ending_at_ram_boundary_does_not_occupy_the_mirror_page() {
        let pc = RAM_SIZE as u32 - 8;
        let mut cache = PagedCache::default();
        let block = cache.insert(pc, Block(2));

        assert!(!cache.invalidate(RAM_SIZE as u32, Some(&block)));
        assert!(Rc::ptr_eq(&cache.get(&cpu_at(pc)).unwrap(), &block));
    }
}
