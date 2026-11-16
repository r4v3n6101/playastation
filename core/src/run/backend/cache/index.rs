use alloc::{boxed::Box, collections::BTreeMap, vec::Vec};
use core::mem;

use crate::machine::interconnect::{BIOS_SIZE, RAM_SIZE, Region, region_of};

const TABLE_BITS: usize = 12;
const SLOTS: usize = 1 << (TABLE_BITS - 2);
const RAM_TABLES: usize = RAM_SIZE >> TABLE_BITS;
const TABLES: usize = RAM_TABLES + (BIOS_SIZE >> TABLE_BITS);

pub(super) struct BlockIndex<V> {
    pages: Box<[Option<Box<Page<V>>>; TABLES]>,
    // Preserve exact keys for unaligned fetches and code outside RAM/BIOS.
    other: BTreeMap<u32, V>,
}

struct Page<V> {
    // Zero is vacant; other values are one-based indices into entries.
    slots: [u16; SLOTS],
    entries: Vec<(u16, V)>,
}

impl<V> Default for BlockIndex<V> {
    fn default() -> Self {
        Self {
            pages: Box::new([const { None }; TABLES]),
            other: BTreeMap::new(),
        }
    }
}

impl<V> BlockIndex<V> {
    pub fn get(&self, pc: &u32) -> Option<&V> {
        let Some(table) = table_of(*pc) else {
            return self.other.get(pc);
        };
        let page = self.pages[table].as_ref()?;
        let index = page.slots[slot_of(*pc)].checked_sub(1)?;
        Some(&page.entries[usize::from(index)].1)
    }

    pub fn insert(&mut self, pc: u32, value: V) -> Option<V> {
        let Some(table) = table_of(pc) else {
            return self.other.insert(pc, value);
        };
        let page = self.pages[table].get_or_insert_with(|| {
            Box::new(Page {
                slots: [0; SLOTS],
                entries: Vec::new(),
            })
        });
        let slot = slot_of(pc);
        if let Some(index) = page.slots[slot].checked_sub(1) {
            return Some(mem::replace(&mut page.entries[usize::from(index)].1, value));
        }
        // A page has at most 1024 aligned instruction addresses, so u16 suffices.
        page.entries.push((slot as u16, value));
        page.slots[slot] = page.entries.len() as u16;
        None
    }

    pub fn remove(&mut self, pc: &u32) -> Option<V> {
        let Some(table) = table_of(*pc) else {
            return self.other.remove(pc);
        };
        let page = self.pages[table].as_mut()?;
        let slot = slot_of(*pc);
        let index = usize::from(page.slots[slot].checked_sub(1)?);
        page.slots[slot] = 0;
        let (_, old) = page.entries.swap_remove(index);
        if let Some((moved_slot, _)) = page.entries.get(index) {
            page.slots[usize::from(*moved_slot)] = (index + 1) as u16;
        }
        // Retain the empty page for repeated invalidation/recompilation.
        Some(old)
    }

    pub fn clear(&mut self) {
        self.pages.iter_mut().for_each(|page| *page = None);
        self.other.clear();
    }
}

fn table_of(pc: u32) -> Option<usize> {
    if pc & 3 != 0 {
        None
    } else if pc < RAM_SIZE as u32 {
        Some(pc as usize >> TABLE_BITS)
    } else if region_of(pc) == Region::Bios {
        Some(RAM_TABLES + ((pc as usize & (BIOS_SIZE - 1)) >> TABLE_BITS))
    } else {
        None
    }
}

fn slot_of(pc: u32) -> usize {
    (pc as usize >> 2) & (SLOTS - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_page_handles_removal_replacement_and_reinsertion() {
        let mut index = BlockIndex::default();
        for slot in 0..SLOTS as u32 {
            assert_eq!(index.insert(slot * 4, slot), None);
        }
        assert_eq!(index.remove(&40), Some(10));
        assert_eq!(index.remove(&40), None);
        assert_eq!(index.insert(40, 12345), None);
        assert_eq!(index.insert(40, 10), Some(12345));
        for slot in 0..SLOTS as u32 {
            assert_eq!(index.get(&(slot * 4)), Some(&slot));
        }
        for slot in (0..SLOTS as u32).rev() {
            assert_eq!(index.remove(&(slot * 4)), Some(slot));
            assert_eq!(index.get(&(slot * 4)), None);
        }
        assert_eq!(index.insert(0, 42), None);
        assert_eq!(index.get(&0), Some(&42));
    }

    #[test]
    fn regions_and_unaligned_addresses_do_not_alias() {
        let keys = [
            0,
            1,
            3,
            4,
            4092,
            4096,
            0x1f00_0000,
            0x1fc0_0000,
            0x1fc7_fffc,
        ];
        let mut index = BlockIndex::default();
        for key in keys {
            assert_eq!(index.insert(key, key), None);
        }
        for key in keys {
            assert_eq!(index.get(&key), Some(&key));
        }
        for key in keys {
            assert_eq!(index.remove(&key), Some(key));
        }
    }

    #[test]
    fn clear_releases_pages_and_fallback_entries() {
        let mut index = BlockIndex::default();
        for key in [0, 4096, 0x1fc0_0000, 1, 0x1f00_0000] {
            index.insert(key, key);
        }
        index.clear();
        assert!(index.pages.iter().all(Option::is_none));
        assert!(index.other.is_empty());
        assert_eq!(index.get(&0), None);
        assert_eq!(index.insert(0, 7), None);
        assert_eq!(index.get(&0), Some(&7));
    }
}
