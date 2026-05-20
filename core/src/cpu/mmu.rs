use core::ops::Range;

// MIPS uses segmented memory, but PSX ignore them and treat all segments as mirror to each other
const KUSEG: Range<u32> = 0x0000_0000..0x8000_0000;
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
    pub fn translate_addr(&self, vaddr: u32) -> TranslationResult {
        match vaddr {
            x if KUSEG.contains(&x) || KSEG0.contains(&x) || KSEG1.contains(&x) => {
                TranslationResult::PhysAddr(x & 0x1FFF_FFFF)
            }
            CACHE_CONTROL => TranslationResult::CacheControl,
            _ => TranslationResult::Unmapped,
        }
    }
}
