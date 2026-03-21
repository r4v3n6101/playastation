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
    AddressLoad { bad_vaddr: u32 } = 0x04,
    AddressStore { bad_vaddr: u32 } = 0x05,
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
    const BAD_VADDR_IDX: usize = 8;
    const STATUS_IDX: usize = 12;
    const CAUSE_IDX: usize = 13;
    const EPC_IDX: usize = 14;

    const STATUS_MODE_BITS_MASK: u32 = 0x3F;
    const STATUS_IEC_BIT: u32 = 1 << 0;
    const STATUS_KUC_BIT: u32 = 1 << 1;
    const STATUS_IEP_BIT: u32 = 1 << 2;
    const STATUS_KUP_BIT: u32 = 1 << 3;
    const STATUS_IEO_BIT: u32 = 1 << 4;
    const STATUS_KUO_BIT: u32 = 1 << 5;
    const STATUS_BEV_BIT: u32 = 1 << 22;
    const CAUSE_EXCCODE_MASK: u32 = 0x1F << 2;
    const CAUSE_IP_MASK: u32 = 0xFF << 8;
    const CAUSE_BD_BIT: u32 = 1 << 31;

    pub fn status(&self) -> u32 {
        self.regs[Self::STATUS_IDX]
    }

    pub fn cause(&self) -> u32 {
        self.regs[Self::CAUSE_IDX]
    }
    pub fn bad_vaddr(&self) -> u32 {
        self.regs[Self::BAD_VADDR_IDX]
    }

    pub fn epc(&self) -> u32 {
        self.regs[Self::EPC_IDX]
    }

    pub fn status_iec(&self) -> bool {
        self.status() & Self::STATUS_IEC_BIT != 0
    }

    pub fn status_kuc(&self) -> bool {
        self.status() & Self::STATUS_KUC_BIT != 0
    }

    pub fn status_iep(&self) -> bool {
        self.status() & Self::STATUS_IEP_BIT != 0
    }

    pub fn status_kup(&self) -> bool {
        self.status() & Self::STATUS_KUP_BIT != 0
    }

    pub fn status_ieo(&self) -> bool {
        self.status() & Self::STATUS_IEO_BIT != 0
    }

    pub fn status_kuo(&self) -> bool {
        self.status() & Self::STATUS_KUO_BIT != 0
    }

    pub fn status_bev(&self) -> bool {
        self.status() & Self::STATUS_BEV_BIT != 0
    }

    pub fn status_mode_bits(&self) -> u32 {
        self.status() & Self::STATUS_MODE_BITS_MASK
    }

    pub fn cause_bd(&self) -> bool {
        self.cause() & Self::CAUSE_BD_BIT != 0
    }

    pub fn cause_ip(&self) -> u32 {
        (self.cause() & Self::CAUSE_IP_MASK) >> 8
    }

    pub fn cause_exc_code(&self) -> u32 {
        (self.cause() & Self::CAUSE_EXCCODE_MASK) >> 2
    }

    pub fn exception_handler(&self) -> u32 {
        if self.status_bev() {
            0xBFC0_0180
        } else {
            0x8000_0080
        }
    }

    pub fn exception_enter(
        &mut self,
        excode: Exception,
        fault_pc: u32,
        in_delay_slot: bool,
        pc: &mut u32,
    ) {
        self.regs[Self::EPC_IDX] = fault_pc;

        let mut cause =
            self.regs[Self::CAUSE_IDX] & !(Self::CAUSE_EXCCODE_MASK | Self::CAUSE_BD_BIT);
        cause |= (excode.discriminant() & 0x1F) << 2;
        if in_delay_slot {
            cause |= Self::CAUSE_BD_BIT;
        }
        self.regs[Self::CAUSE_IDX] = cause;

        if let Exception::AddressLoad { bad_vaddr } | Exception::AddressStore { bad_vaddr } = excode
        {
            self.regs[Self::BAD_VADDR_IDX] = bad_vaddr;
        }

        let sr = &mut self.regs[Self::STATUS_IDX];
        *sr = (*sr & !Self::STATUS_MODE_BITS_MASK)
            | ((*sr & Self::STATUS_MODE_BITS_MASK) << 2 & Self::STATUS_MODE_BITS_MASK);

        // Jump to exception handler
        *pc = self.exception_handler();
    }

    pub fn exception_leave(&mut self, pc: &mut u32) {
        let sr = &mut self.regs[Self::STATUS_IDX];
        *sr = (*sr & !Self::STATUS_MODE_BITS_MASK) | (*sr & Self::STATUS_MODE_BITS_MASK) >> 2;

        // Jump back
        *pc = self.regs[Self::EPC_IDX];
    }
}
