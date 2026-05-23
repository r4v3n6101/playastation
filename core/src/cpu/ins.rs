#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Instruction {
    Sll { rt: u8, rd: u8, shamt: u8 },
    Srl { rt: u8, rd: u8, shamt: u8 },
    Sra { rt: u8, rd: u8, shamt: u8 },
    Sllv { rs: u8, rt: u8, rd: u8 },
    Srlv { rs: u8, rt: u8, rd: u8 },
    Srav { rs: u8, rt: u8, rd: u8 },
    Jr { rs: u8 },
    Jalr { rs: u8, rd: u8 },
    Syscall { code: u32 },
    Break { code: u32 },
    Mfhi { rd: u8 },
    Mthi { rs: u8 },
    Mflo { rd: u8 },
    Mtlo { rs: u8 },
    Mult { rs: u8, rt: u8 },
    Multu { rs: u8, rt: u8 },
    Div { rs: u8, rt: u8 },
    Divu { rs: u8, rt: u8 },
    Add { rs: u8, rt: u8, rd: u8 },
    Addu { rs: u8, rt: u8, rd: u8 },
    Sub { rs: u8, rt: u8, rd: u8 },
    Subu { rs: u8, rt: u8, rd: u8 },
    And { rs: u8, rt: u8, rd: u8 },
    Or { rs: u8, rt: u8, rd: u8 },
    Xor { rs: u8, rt: u8, rd: u8 },
    Nor { rs: u8, rt: u8, rd: u8 },
    Slt { rs: u8, rt: u8, rd: u8 },
    Sltu { rs: u8, rt: u8, rd: u8 },
    Bltz { rs: u8, imm_sext: i16 },
    Bgez { rs: u8, imm_sext: i16 },
    Bltzal { rs: u8, imm_sext: i16 },
    Bgezal { rs: u8, imm_sext: i16 },
    Mfc0 { rt: u8, cop0_reg: u8 },
    Mtc0 { rt: u8, cop0_reg: u8 },
    Cfc0 { rt: u8, cop0_reg: u8 },
    Ctc0 { rt: u8, cop0_reg: u8 },
    Rfe,
    J { target: u32 },
    Jal { target: u32 },
    Beq { rs: u8, rt: u8, imm_sext: i16 },
    Bne { rs: u8, rt: u8, imm_sext: i16 },
    Blez { rs: u8, imm_sext: i16 },
    Bgtz { rs: u8, imm_sext: i16 },
    Addi { rs: u8, rt: u8, imm_sext: i16 },
    Addiu { rs: u8, rt: u8, imm_sext: i16 },
    Slti { rs: u8, rt: u8, imm_sext: i16 },
    Sltiu { rs: u8, rt: u8, imm_sext: i16 },
    Andi { rs: u8, rt: u8, imm: u16 },
    Ori { rs: u8, rt: u8, imm: u16 },
    Xori { rs: u8, rt: u8, imm: u16 },
    Lui { rt: u8, imm: u16 },
    Lb { rs: u8, rt: u8, imm_sext: i16 },
    Lh { rs: u8, rt: u8, imm_sext: i16 },
    Lwl { rs: u8, rt: u8, imm_sext: i16 },
    Lw { rs: u8, rt: u8, imm_sext: i16 },
    Lbu { rs: u8, rt: u8, imm_sext: i16 },
    Lhu { rs: u8, rt: u8, imm_sext: i16 },
    Lwr { rs: u8, rt: u8, imm_sext: i16 },
    Sb { rs: u8, rt: u8, imm_sext: i16 },
    Sh { rs: u8, rt: u8, imm_sext: i16 },
    Swl { rs: u8, rt: u8, imm_sext: i16 },
    Sw { rs: u8, rt: u8, imm_sext: i16 },
    Swr { rs: u8, rt: u8, imm_sext: i16 },
}

impl Instruction {
    pub fn decode(ins: u32) -> Option<Self> {
        let rs = ((ins >> 21) & 0x1F) as u8;
        let rt = ((ins >> 16) & 0x1F) as u8;
        let rd = ((ins >> 11) & 0x1F) as u8;
        let shamt = ((ins >> 6) & 0x1F) as u8;
        let imm = (ins & 0xFFFF) as u16;
        let imm_sext = imm.cast_signed();
        let target = ins & 0x03FF_FFFF;

        Some(match ins >> 26 {
            // SPECIAL
            0x00 => match ins & 0x3F {
                0x00 => Self::Sll { rt, rd, shamt },
                0x02 => Self::Srl { rt, rd, shamt },
                0x03 => Self::Sra { rt, rd, shamt },
                0x04 => Self::Sllv { rs, rt, rd },
                0x06 => Self::Srlv { rs, rt, rd },
                0x07 => Self::Srav { rs, rt, rd },
                0x08 => Self::Jr { rs },
                0x09 => Self::Jalr { rs, rd },
                0x0C => Self::Syscall {
                    code: (ins >> 6) & 0x000F_FFFF,
                },
                0x0D => Self::Break {
                    code: (ins >> 6) & 0x000F_FFFF,
                },
                0x10 => Self::Mfhi { rd },
                0x11 => Self::Mthi { rs },
                0x12 => Self::Mflo { rd },
                0x13 => Self::Mtlo { rs },
                0x18 => Self::Mult { rs, rt },
                0x19 => Self::Multu { rs, rt },
                0x1A => Self::Div { rs, rt },
                0x1B => Self::Divu { rs, rt },
                0x20 => Self::Add { rs, rt, rd },
                0x21 => Self::Addu { rs, rt, rd },
                0x22 => Self::Sub { rs, rt, rd },
                0x23 => Self::Subu { rs, rt, rd },
                0x24 => Self::And { rs, rt, rd },
                0x25 => Self::Or { rs, rt, rd },
                0x26 => Self::Xor { rs, rt, rd },
                0x27 => Self::Nor { rs, rt, rd },
                0x2A => Self::Slt { rs, rt, rd },
                0x2B => Self::Sltu { rs, rt, rd },
                _ => return None,
            },

            // REGIMM. Only the canonical link encodings link on the PSX CPU;
            // the other rt values alias to BLTZ/BGEZ by their low bit.
            0x01 => match rt {
                0x10 => Self::Bltzal { rs, imm_sext },
                0x11 => Self::Bgezal { rs, imm_sext },
                rt if rt & 1 == 0 => Self::Bltz { rs, imm_sext },
                _ => Self::Bgez { rs, imm_sext },
            },

            0x02 => Self::J { target },
            0x03 => Self::Jal { target },
            0x04 => Self::Beq { rs, rt, imm_sext },
            0x05 => Self::Bne { rs, rt, imm_sext },
            0x06 => Self::Blez { rs, imm_sext },
            0x07 => Self::Bgtz { rs, imm_sext },
            0x08 => Self::Addi { rs, rt, imm_sext },
            0x09 => Self::Addiu { rs, rt, imm_sext },
            0x0A => Self::Slti { rs, rt, imm_sext },
            0x0B => Self::Sltiu { rs, rt, imm_sext },
            0x0C => Self::Andi { rs, rt, imm },
            0x0D => Self::Ori { rs, rt, imm },
            0x0E => Self::Xori { rs, rt, imm },
            0x0F => Self::Lui { rt, imm },

            // COP0
            0x10 => match (ins >> 21) & 0x1F {
                0x00 => Self::Mfc0 { rt, cop0_reg: rd },
                0x02 => Self::Cfc0 { rt, cop0_reg: rd },
                0x04 => Self::Mtc0 { rt, cop0_reg: rd },
                0x06 => Self::Ctc0 { rt, cop0_reg: rd },
                0x10 if ins & 0x3F == 0x10 => Self::Rfe,
                _ => return None,
            },

            0x20 => Self::Lb { rs, rt, imm_sext },
            0x21 => Self::Lh { rs, rt, imm_sext },
            0x22 => Self::Lwl { rs, rt, imm_sext },
            0x23 => Self::Lw { rs, rt, imm_sext },
            0x24 => Self::Lbu { rs, rt, imm_sext },
            0x25 => Self::Lhu { rs, rt, imm_sext },
            0x26 => Self::Lwr { rs, rt, imm_sext },
            0x28 => Self::Sb { rs, rt, imm_sext },
            0x29 => Self::Sh { rs, rt, imm_sext },
            0x2A => Self::Swl { rs, rt, imm_sext },
            0x2B => Self::Sw { rs, rt, imm_sext },
            0x2E => Self::Swr { rs, rt, imm_sext },

            _ => return None,
        })
    }

    pub fn has_branch_delay(self) -> bool {
        matches!(
            self,
            Self::J { .. }
                | Self::Jal { .. }
                | Self::Jr { .. }
                | Self::Jalr { .. }
                | Self::Beq { .. }
                | Self::Bne { .. }
                | Self::Blez { .. }
                | Self::Bgtz { .. }
                | Self::Bltz { .. }
                | Self::Bgez { .. }
                | Self::Bltzal { .. }
                | Self::Bgezal { .. }
        )
    }
}
