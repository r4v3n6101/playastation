pub use cop0::{Cop0, Exception};
pub use ins::Opcode;

mod cop0;
mod ins;

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
    /// Pending jump (aka branch delay slot).
    pub pending_jump: PendingJump,

    // Soprocessors
    pub cop0: Cop0,
}

#[derive(Debug, Default, Copy, Clone)]
pub struct PendingLoad {
    /// Where write to value. Zero ignores any write.
    pub dest: usize,
    /// Loaded value.
    pub value: u32,
}

#[derive(Debug, Default, Copy, Clone)]
pub struct PendingJump {
    /// Whether a branch/jump was seen.
    /// Whether a jump will happen.
    pub happen: bool,
    /// Jump target.
    pub target: u32,
}

/// Reset state of the CPU.
impl Default for Cpu {
    fn default() -> Self {
        Self {
            gpr: [0; _],
            pc: 0xBFC0_0000,
            hi: 0,
            lo: 0,

            pending_load: PendingLoad { dest: 0, value: 0 },
            pending_jump: PendingJump {
                happen: false,
                target: 0,
            },

            cop0: Cop0::default(),
        }
    }
}

impl Cpu {
    pub const DEFAULT_LINK_REG: usize = 31;
}
