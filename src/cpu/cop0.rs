use modular_bitfield::prelude::*;
use strum::{EnumDiscriminants, IntoDiscriminant};

#[bitfield(bits = 32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    // IE/KU stack
    pub iec: bool,
    pub kuc: bool,
    pub iep: bool,
    pub kup: bool,
    pub ieo: bool,
    pub kuo: bool,
    #[skip]
    reserved: B2,
    /// Interrupt mask
    pub im: B8,
    /// Isolate cache
    pub isc: bool,
    #[skip]
    reserved: B5,
    /// Boot vector select
    pub bev: bool,
    #[skip]
    reserved: B9,
}

#[bitfield(bits = 32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cause {
    #[skip]
    reserved: B2,
    pub excode: B5,
    #[skip]
    reserved: B1,
    // Interrupt pending
    pub ip: B8,
    #[skip]
    reserved: B15,
    // Branch delay flag
    pub bd: bool,
}

/// Simplified Cop0 (coprocessor 0) with the logic used in PSX.
/// It's not fully implemented, because PSX doesn't use TLB for example.
#[derive(Debug, Copy, Clone)]
pub struct Cop0 {
    pub regs: [u32; 32],
}

#[derive(EnumDiscriminants, Debug, Copy, Clone, PartialEq, Eq)]
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
        Status::from_bytes(self.regs[Self::STATUS_IDX].to_le_bytes())
    }

    pub fn cause(&self) -> Cause {
        Cause::from_bytes(self.regs[Self::CAUSE_IDX].to_le_bytes())
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
        cause.set_excode(exception.discriminant() as u8);
        self.regs[Self::CAUSE_IDX] = u32::from_le_bytes(cause.into_bytes());

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

        self.regs[Self::CAUSE_IDX] = u32::from_le_bytes(cause.into_bytes());
    }
}
