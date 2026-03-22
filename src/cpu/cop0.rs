/// Simplified Cop0 (coprocessor 0) with the logic used in PSX.
/// It's not fully implemented, because PSX doesn't use TLB for example.
#[derive(Default, Debug)]
pub struct Cop0 {
    pub regs: [u32; 32],
}

#[derive(Debug, Copy, Clone)]
#[repr(u32)]
pub enum Exception {
    Interrupt = 0x00,
    UnalignedLoad { bad_vaddr: u32 } = 0x04,
    UnalignedStore { bad_vaddr: u32 } = 0x05,
    InstructionBus { bad_vaddr: u32 } = 0x06,
    DataBus { bad_vaddr: u32 } = 0x07,
    Syscall = 0x08,
    Break = 0x09,
    ReservedInstruction = 0x0A,
    Overflow = 0x0C,
}

impl Exception {
    fn discriminant(&self) -> u32 {
        // https://doc.rust-lang.org/std/mem/fn.discriminant.html#accessing-the-numeric-value-of-the-discriminant
        unsafe { *<*const _>::from(self).cast::<u32>() }
    }
}

impl Cop0 {
    pub const BAD_VADDR_IDX: usize = 8;
    pub const STATUS_IDX: usize = 12;
    pub const CAUSE_IDX: usize = 13;
    pub const EPC_IDX: usize = 14;

    pub fn status_iec(&self) -> bool {
        self.regs[Self::STATUS_IDX] & 0x1 != 0
    }

    pub fn status_kuc(&self) -> bool {
        self.regs[Self::STATUS_IDX] & 0x2 != 0
    }

    pub fn status_iep(&self) -> bool {
        self.regs[Self::STATUS_IDX] & 0x4 != 0
    }

    pub fn status_kup(&self) -> bool {
        self.regs[Self::STATUS_IDX] & 0x8 != 0
    }

    pub fn status_ieo(&self) -> bool {
        self.regs[Self::STATUS_IDX] & 0x10 != 0
    }

    pub fn status_kuo(&self) -> bool {
        self.regs[Self::STATUS_IDX] & 0x20 != 0
    }

    pub fn status_bev(&self) -> bool {
        self.regs[Self::STATUS_IDX] & 0x400000 != 0
    }

    pub fn status_mode_bits(&self) -> u32 {
        self.regs[Self::STATUS_IDX] & 0b111111
    }

    pub fn cause_bd(&self) -> bool {
        self.regs[Self::CAUSE_IDX] & 0x80000000 != 0
    }

    pub fn cause_ip(&self) -> u32 {
        (self.regs[Self::CAUSE_IDX] & 0xFF00) >> 8
    }

    pub fn cause_exc_code(&self) -> u32 {
        (self.regs[Self::CAUSE_IDX] & 0b1111100) >> 2
    }

    pub fn exception_handler(&self) -> u32 {
        if self.status_bev() {
            0xBFC0_0180
        } else {
            0x8000_0080
        }
    }

    /// Push IEc/KUc to IEp/KUp and IEp/KUp to IEo/KUo, then clear current mode
    pub fn exception_enter(&mut self, excode: Exception, epc: u32, in_delay_slot: bool) {
        self.regs[Self::EPC_IDX] = epc;

        let mut cause = self.regs[Self::CAUSE_IDX] & !((0b11111 << 2) | (1 << 31));
        cause |= (excode.discriminant() & 0b11111) << 2;
        cause |= (in_delay_slot as u32) << 31;
        self.regs[Self::CAUSE_IDX] = cause;

        if let Exception::UnalignedLoad { bad_vaddr }
        | Exception::UnalignedStore { bad_vaddr }
        | Exception::InstructionBus { bad_vaddr }
        | Exception::DataBus { bad_vaddr } = excode
        {
            self.regs[Self::BAD_VADDR_IDX] = bad_vaddr;
        }

        let sr = &mut self.regs[Self::STATUS_IDX];
        *sr = (*sr & !0x3F) | (((*sr & 0x3F) << 2) & 0x3F);
    }

    /// Pop the status stack: restore IEc/KUc from IEp/KUp and IEp/KUp from IEo/KUo
    pub fn exception_leave(&mut self) {
        let sr = &mut self.regs[Self::STATUS_IDX];
        let low6 = *sr & 0b111111;
        *sr = (*sr & !0b111111) | (low6 & 0b110000) | ((low6 >> 2) & 0b1111);
    }
}
