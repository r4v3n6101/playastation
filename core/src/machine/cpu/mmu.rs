use core::ops::Range;

/// KUSEG spans 0x0000_0000..0x8000_0000.
/// On PSX, accesses to 0x2000_0000..0x8000_0000 raise an exception.
const KUSEG: Range<u32> = 0x0000_0000..0x2000_0000;
const KSEG0: Range<u32> = 0x8000_0000..0xA000_0000;
const KSEG1: Range<u32> = 0xA000_0000..0xC000_0000;

/// Special KSEG2 address that bypasses the bus and works with the cpu directly
const CACHE_CONTROL: u32 = 0xFFFE_0130;

pub enum TranslationResult {
    PhysAddr(u32),
    CacheControl,
    Unmapped,
}

/// Stripped PSX MMU that doesn't use TLB and primarily doing only address translating.
#[derive(Debug, Copy, Clone)]
pub struct Mmu;

impl Mmu {
    /// Translate a virtual address from segments into physical one.
    #[inline(always)]
    pub fn translate_addr(&self, vaddr: u32) -> TranslationResult {
        match self.mapped_segment(vaddr) {
            Some(segment) => TranslationResult::PhysAddr(vaddr - segment.start),
            None if vaddr == CACHE_CONTROL => TranslationResult::CacheControl,
            None => TranslationResult::Unmapped,
        }
    }

    #[inline(always)]
    pub(crate) fn mapped_segment(&self, vaddr: u32) -> Option<Range<u32>> {
        match vaddr {
            x if KUSEG.contains(&x) => Some(KUSEG),
            x if KSEG0.contains(&x) => Some(KSEG0),
            x if KSEG1.contains(&x) => Some(KSEG1),
            _ => None,
        }
    }
}
