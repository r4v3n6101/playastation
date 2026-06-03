use crate::interconnect::Bus;

pub use cop0::{Cop0, Exception};
pub use ins::Instruction;
pub use mmu::{Mmu, TranslationResult};

mod cop0;
mod ins;
mod mmu;

#[derive(Debug, Copy, Clone)]
pub struct Cpu {
    /// General purpose registers.
    pub gpr: [u32; 32],
    /// Program counter.
    pub pc: u32,
    /// High bits part for mul/div ops.
    pub hi: u32,
    /// Low bits part for mul/div ops.
    pub lo: u32,

    /// Pending load from RAM (aka load-delay slot).
    pub pending_load: PendingLoad,
    /// Pending jump that occurs after branch-delay slot.
    pub pending_jump: PendingJump,

    /// MMU for address translating.
    pub mmu: Mmu,

    // Coprocessors
    pub cop0: Cop0,
}

#[derive(Debug, Default, Copy, Clone)]
pub struct PendingLoad {
    /// Where write to value. Zero ignores any write.
    pub dest: u8,
    /// Loaded value.
    pub value: u32,
}

#[derive(Debug, Default, Copy, Clone)]
pub struct PendingJump {
    /// Whether branch/jump operation occurred.
    pub valid: bool,
    /// Whether to jump to target.
    pub cond: bool,
    /// Address to jump to if the branch is taken.
    pub then: u32,
    /// Address of instruction after delay slot (if no jump taken).
    pub otherwise: u32,
}

/// Reset state of the CPU.
impl Default for Cpu {
    fn default() -> Self {
        Self {
            gpr: [0; _],
            pc: 0xBFC0_0000,
            hi: 0,
            lo: 0,

            pending_load: PendingLoad::default(),
            pending_jump: PendingJump::default(),

            mmu: Mmu,

            cop0: Cop0::default(),
        }
    }
}

impl PendingJump {
    pub fn target(&self) -> u32 {
        if self.cond { self.then } else { self.otherwise }
    }
}

impl Cpu {
    pub const DEFAULT_LINK_REG: u8 = 31;

    #[inline(always)]
    pub fn read_bus<const N: usize>(
        &mut self,
        bus: &mut Bus,
        vaddr: u32,
    ) -> Result<[u8; N], Exception> {
        if !vaddr.is_multiple_of(N as u32) {
            return Err(Exception::UnalignedLoad { bad_vaddr: vaddr });
        }

        let paddr = match self.mmu.translate_addr(vaddr) {
            TranslationResult::PhysAddr(res) => res,
            TranslationResult::CacheControl => return Ok([0; _]),
            TranslationResult::Unmapped => return Err(Exception::DataBus { bad_vaddr: vaddr }),
        };

        Ok(bus.load(paddr))
    }

    #[inline(always)]
    pub fn write_bus<const N: usize>(
        &mut self,
        bus: &mut Bus,
        vaddr: u32,
        val: [u8; N],
    ) -> Result<(), Exception> {
        if !vaddr.is_multiple_of(N as u32) {
            return Err(Exception::UnalignedStore { bad_vaddr: vaddr });
        }

        let paddr = match self.mmu.translate_addr(vaddr) {
            TranslationResult::PhysAddr(res) => res,
            TranslationResult::CacheControl => return Ok(()),
            TranslationResult::Unmapped => return Err(Exception::DataBus { bad_vaddr: vaddr }),
        };

        bus.store(paddr, val);

        Ok(())
    }
}
