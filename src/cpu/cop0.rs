use strum::{EnumDiscriminants, IntoDiscriminant};
use tock_registers::LocalRegisterCopy;

tock_registers::register_bitfields![u32,
    pub Status [
        IEC OFFSET(0) NUMBITS(1) [],
        KUC OFFSET(1) NUMBITS(1) [],

        IEP OFFSET(2) NUMBITS(1) [],
        KUP OFFSET(3) NUMBITS(1) [],

        IEO OFFSET(4) NUMBITS(1) [],
        KUO OFFSET(5) NUMBITS(1) [],

        IM OFFSET(8) NUMBITS(8) [],

        BEV OFFSET(22) NUMBITS(1) []
    ],

    pub Cause [
        EXCCODE OFFSET(2) NUMBITS(5) [],

        IP OFFSET(8) NUMBITS(8) [
            SW0 = 0,
            SW1 = 1,
            HW = 2,
        ],

        BD OFFSET(31) NUMBITS(1) []
    ]
];

/// Simplified Cop0 (coprocessor 0) with the logic used in PSX.
/// It's not fully implemented, because PSX doesn't use TLB for example.
#[derive(Default, Debug)]
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

impl Cop0 {
    pub const BAD_VADDR_IDX: usize = 8;
    pub const STATUS_IDX: usize = 12;
    pub const CAUSE_IDX: usize = 13;
    pub const EPC_IDX: usize = 14;

    pub fn status_reg(&self) -> LocalRegisterCopy<u32, Status::Register> {
        LocalRegisterCopy::new(self.regs[Self::STATUS_IDX])
    }

    pub fn cause_reg(&self) -> LocalRegisterCopy<u32, Cause::Register> {
        LocalRegisterCopy::new(self.regs[Self::CAUSE_IDX])
    }

    pub fn exception_handler(&self) -> u32 {
        let status = self.status_reg();

        if status.is_set(Status::BEV) {
            0xBFC0_0180
        } else {
            0x8000_0080
        }
    }

    /// Push IEc/KUc to IEp/KUp and IEp/KUp to IEo/KUo, then clear current mode
    pub fn exception_enter(&mut self, excode: Exception, fault_pc: u32, in_delay_slot: bool) {
        self.regs[Self::EPC_IDX] = if in_delay_slot {
            fault_pc.wrapping_sub(4)
        } else {
            fault_pc
        };

        let cause = &mut self.regs[Self::CAUSE_IDX];
        *cause &= !((0b11111 << 2) | (1 << 31));
        *cause |= (excode.discriminant() as u32 & 0b11111) << 2;
        *cause |= u32::from(in_delay_slot) << 31;

        if let Exception::UnalignedLoad { bad_vaddr }
        | Exception::UnalignedStore { bad_vaddr }
        | Exception::InstructionBus { bad_vaddr }
        | Exception::DataBus { bad_vaddr } = excode
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
        let status = self.status_reg();
        let cause = self.cause_reg();

        status.is_set(Status::IEC) && ((cause.read(Cause::IP) & status.read(Status::IM)) != 0)
    }

    pub fn update_irq_line(&mut self, pending: bool) {
        let mut cause = self.cause_reg();

        if pending {
            cause.modify(Cause::IP::HW);
        } else {
            cause.modify(Cause::IP::CLEAR);
        }

        self.regs[Self::CAUSE_IDX] = cause.get();
    }
}
