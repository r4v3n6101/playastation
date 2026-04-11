use strum::FromRepr;

#[derive(Debug, Copy, Clone, FromRepr)]
#[repr(u16)]
pub enum Opcode {
    /// Shift left logical (shamt).
    Sll = 0x00_00,
    /// Shift right logical (shamt).
    Srl = 0x02_00,
    /// Shift right arithmetic (shamt).
    Sra = 0x03_00,
    /// Shift left logical (var).
    Sllv = 0x04_00,
    /// Shift right logical (var).
    Srlv = 0x06_00,
    /// Shift right arithmetic (var).
    Srav = 0x07_00,
    /// Jump register.
    Jr = 0x08_00,
    /// Jump and link register.
    Jalr = 0x09_00,
    /// Syscall trap.
    Syscall = 0x0C_00,
    /// Breakpoint trap.
    Break = 0x0D_00,
    /// Move from HI.
    Mfhi = 0x10_00,
    /// Move to HI.
    Mthi = 0x11_00,
    /// Move from LO.
    Mflo = 0x12_00,
    /// Move to LO.
    Mtlo = 0x13_00,
    /// Multiply (signed) -> HI/LO.
    Mult = 0x18_00,
    /// Multiply (unsigned) -> HI/LO.
    Multu = 0x19_00,
    /// Divide (signed) -> HI/LO.
    Div = 0x1A_00,
    /// Divide (unsigned) -> HI/LO.
    Divu = 0x1B_00,
    /// Add (signed, overflow).
    Add = 0x20_00,
    /// Add unsigned (no overflow).
    Addu = 0x21_00,
    /// Subtract (signed, overflow).
    Sub = 0x22_00,
    /// Subtract unsigned (no overflow).
    Subu = 0x23_00,
    /// Bitwise and.
    And = 0x24_00,
    /// Bitwise or.
    Or = 0x25_00,
    /// Bitwise xor.
    Xor = 0x26_00,
    /// Bitwise nor.
    Nor = 0x27_00,
    /// Set on less than (signed).
    Slt = 0x2A_00,
    /// Set on less than (unsigned).
    Sltu = 0x2B_00,

    // REGIMM (opcode 0x01, tag uses rt field)
    /// Branch on < 0.
    Bltz = 0x00_01,
    /// Branch on >= 0.
    Bgez = 0x01_01,
    /// Branch on < 0 and link.
    Bltzal = 0x10_01,
    /// Branch on >= 0 and link.
    Bgezal = 0x11_01,

    /// Move from coprocessor 0.
    Mfc0 = 0x00_10,
    /// Move to coprocessor 0.
    Mtc0 = 0x04_10,
    /// Move from coprocessor 0 control.
    Cfc0 = 0x02_10,
    /// Move to coprocessor 0 control.
    Ctc0 = 0x06_10,
    /// Return from exception.
    Rfe = 0x10_10,

    /// Jump.
    J = 0x00_02,
    /// Jump and link.
    Jal = 0x00_03,
    /// Branch on equal.
    Beq = 0x00_04,
    /// Branch on not equal.
    Bne = 0x00_05,
    /// Branch on <= 0.
    Blez = 0x00_06,
    /// Branch on > 0.
    Bgtz = 0x00_07,
    /// Add immediate (signed, overflow).
    Addi = 0x00_08,
    /// Add immediate unsigned (no overflow).
    Addiu = 0x00_09,
    /// Set on less than immediate (signed).
    Slti = 0x00_0A,
    /// Set on less than immediate (unsigned).
    Sltiu = 0x00_0B,
    /// Bitwise and immediate.
    Andi = 0x00_0C,
    /// Bitwise or immediate.
    Ori = 0x00_0D,
    /// Bitwise xor immediate.
    Xori = 0x00_0E,
    /// Load upper immediate.
    Lui = 0x00_0F,

    /// Load byte (signed).
    Lb = 0x00_20,
    /// Load halfword (signed).
    Lh = 0x00_21,
    /// Load word left.
    Lwl = 0x00_22,
    /// Load word.
    Lw = 0x00_23,
    /// Load byte (unsigned).
    Lbu = 0x00_24,
    /// Load halfword (unsigned).
    Lhu = 0x00_25,
    /// Load word right.
    Lwr = 0x00_26,

    /// Store byte.
    Sb = 0x00_28,
    /// Store halfword.
    Sh = 0x00_29,
    /// Store word left.
    Swl = 0x00_2A,
    /// Store word.
    Sw = 0x00_2B,
    /// Store word right.
    Swr = 0x00_2E,
}

impl Opcode {
    pub fn decode(ins: u32) -> Option<Self> {
        let opcode = (ins >> 26) as u16;
        let rs = ((ins >> 21) & 0x1F) as usize;
        let rt = ((ins >> 16) & 0x1F) as usize;
        let funct = ins & 0x3F;
        let tag = match opcode {
            0x00 => ((funct as u16) << 8) | opcode,
            0x01 => ((rt as u16) << 8) | opcode,
            0x10 => ((rs as u16) << 8) | opcode,
            _ => opcode,
        };
        Self::from_repr(tag)
    }

    pub fn has_branch_delay(self) -> bool {
        matches!(
            self,
            Self::J
                | Self::Jal
                | Self::Jr
                | Self::Jalr
                | Self::Beq
                | Self::Bne
                | Self::Blez
                | Self::Bgtz
                | Self::Bltz
                | Self::Bgez
                | Self::Bltzal
                | Self::Bgezal
        )
    }
}
