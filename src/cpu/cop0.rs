use strum::{EnumDiscriminants, IntoDiscriminant};

bitfield::bitfield! {
    #[derive(Clone, Copy, Default, PartialEq, Eq)]
    pub struct Status(u32);
    impl Debug;

    // IE/KU stack
    pub iec, _: 0;
    pub kuc, _: 1;
    pub iep, _: 2;
    pub kup, _: 3;
    pub ieo, _: 4;
    pub kuo, _: 5;

    /// Interrupt mask
    pub im, _: 15, 8;

    /// Isolate cache
    pub isc, _: 16;

    /// Boot vector select
    pub bev, _: 22;
}

bitfield::bitfield! {
    #[derive(Clone, Copy, Default, PartialEq, Eq)]
    pub struct Cause(u32);
    impl Debug;

    pub excode, set_excode: 6, 2;

    // Interrupt pending
    pub ip, set_ip: 15, 8;

    // Branch delay flag
    pub bd, set_bd: 31;
}

/// Simplified Cop0 (coprocessor 0) with the logic used in PSX.
/// It's not fully implemented, because PSX doesn't use TLB for example.
#[derive(Debug, Copy, Clone)]
pub struct Cop0 {
    pub regs: [u32; 32],
}

#[derive(EnumDiscriminants, Debug, Copy, Clone)]
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

impl Default for Cop0 {
    fn default() -> Self {
        let mut regs = <[_; _]>::default();

        // Status.BEV = 1, everything else 0
        regs[Self::STATUS_IDX] = 0x0040_0000;

        Self { regs }
    }
}

impl Cop0 {
    pub const BAD_VADDR_IDX: usize = 8;
    pub const STATUS_IDX: usize = 12;
    pub const CAUSE_IDX: usize = 13;
    pub const EPC_IDX: usize = 14;

    pub fn status(&self) -> Status {
        Status(self.regs[Self::STATUS_IDX])
    }

    pub fn cause(&self) -> Cause {
        Cause(self.regs[Self::CAUSE_IDX])
    }

    pub fn exception_handler(&self) -> u32 {
        if self.status().bev() {
            0xBFC0_0180
        } else {
            0x8000_0080
        }
    }

    /// Push IEc/KUc to IEp/KUp and IEp/KUp to IEo/KUo, then clear current mode
    pub fn exception_enter(&mut self, exception: Exception, fault_pc: u32, in_delay_slot: bool) {
        self.regs[Self::EPC_IDX] = if in_delay_slot {
            fault_pc.wrapping_sub(4)
        } else {
            fault_pc
        };

        let mut cause = self.cause();
        cause.set_bd(in_delay_slot);
        cause.set_excode(exception.discriminant() as u32);
        self.regs[Self::CAUSE_IDX] = cause.0;

        if let Exception::UnalignedLoad { bad_vaddr }
        | Exception::UnalignedStore { bad_vaddr }
        | Exception::InstructionBus { bad_vaddr }
        | Exception::DataBus { bad_vaddr } = exception
        {
            self.regs[Self::BAD_VADDR_IDX] = bad_vaddr;
        }

        let sr = &mut self.regs[Self::STATUS_IDX];
        *sr = (*sr & !0b111111) | (((*sr & 0b111111) << 2) & 0b111111);
    }

    /// Pop the status stack: restore IEc/KUc from IEp/KUp and IEp/KUp from IEo/KUo
    pub fn exception_leave(&mut self) {
        let sr = &mut self.regs[Self::STATUS_IDX];
        let low6 = *sr & 0b111111;
        *sr = (*sr & !0b111111) | (low6 & 0b110000) | ((low6 >> 2) & 0b1111);
    }

    pub fn interrupt_pending(&self) -> bool {
        let iec = self.status().iec();
        let ip = self.cause().ip();
        let im = self.status().im();

        iec && ((ip & im) != 0)
    }

    pub fn set_hw_irq(&mut self, active: bool) {
        let mut cause = self.cause();
        let mut ip = cause.ip();
        if active {
            // PSX supports only second HW lane (bit 2)
            ip |= 1 << 2;
        } else {
            ip &= !(1 << 2);
        }
        cause.set_ip(ip);

        self.regs[Self::CAUSE_IDX] = cause.0;
    }
}
