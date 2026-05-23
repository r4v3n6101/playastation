#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Instruction {
    Sll { rt: usize, rd: usize, shamt: u32 },
    Srl { rt: usize, rd: usize, shamt: u32 },
    Sra { rt: usize, rd: usize, shamt: u32 },
    Sllv { rs: usize, rt: usize, rd: usize },
    Srlv { rs: usize, rt: usize, rd: usize },
    Srav { rs: usize, rt: usize, rd: usize },
    Jr { rs: usize },
    Jalr { rs: usize, rd: usize },
    Syscall { code: u32 },
    Break { code: u32 },
    Mfhi { rd: usize },
    Mthi { rs: usize },
    Mflo { rd: usize },
    Mtlo { rs: usize },
    Mult { rs: usize, rt: usize },
    Multu { rs: usize, rt: usize },
    Div { rs: usize, rt: usize },
    Divu { rs: usize, rt: usize },
    Add { rs: usize, rt: usize, rd: usize },
    Addu { rs: usize, rt: usize, rd: usize },
    Sub { rs: usize, rt: usize, rd: usize },
    Subu { rs: usize, rt: usize, rd: usize },
    And { rs: usize, rt: usize, rd: usize },
    Or { rs: usize, rt: usize, rd: usize },
    Xor { rs: usize, rt: usize, rd: usize },
    Nor { rs: usize, rt: usize, rd: usize },
    Slt { rs: usize, rt: usize, rd: usize },
    Sltu { rs: usize, rt: usize, rd: usize },
    Bltz { rs: usize, imm_sext: i32 },
    Bgez { rs: usize, imm_sext: i32 },
    Bltzal { rs: usize, imm_sext: i32 },
    Bgezal { rs: usize, imm_sext: i32 },
    Mfc0 { rt: usize, cop0_reg: usize },
    Mtc0 { rt: usize, cop0_reg: usize },
    Cfc0 { rt: usize, cop0_reg: usize },
    Ctc0 { rt: usize, cop0_reg: usize },
    Rfe,
    J { target: u32 },
    Jal { target: u32 },
    Beq { rs: usize, rt: usize, imm_sext: i32 },
    Bne { rs: usize, rt: usize, imm_sext: i32 },
    Blez { rs: usize, imm_sext: i32 },
    Bgtz { rs: usize, imm_sext: i32 },
    Addi { rs: usize, rt: usize, imm_sext: i32 },
    Addiu { rs: usize, rt: usize, imm_sext: i32 },
    Slti { rs: usize, rt: usize, imm_sext: i32 },
    Sltiu { rs: usize, rt: usize, imm_sext: i32 },
    Andi { rs: usize, rt: usize, imm: u32 },
    Ori { rs: usize, rt: usize, imm: u32 },
    Xori { rs: usize, rt: usize, imm: u32 },
    Lui { rt: usize, imm: u32 },
    Lb { rs: usize, rt: usize, imm_sext: i32 },
    Lh { rs: usize, rt: usize, imm_sext: i32 },
    Lwl { rs: usize, rt: usize, imm_sext: i32 },
    Lw { rs: usize, rt: usize, imm_sext: i32 },
    Lbu { rs: usize, rt: usize, imm_sext: i32 },
    Lhu { rs: usize, rt: usize, imm_sext: i32 },
    Lwr { rs: usize, rt: usize, imm_sext: i32 },
    Sb { rs: usize, rt: usize, imm_sext: i32 },
    Sh { rs: usize, rt: usize, imm_sext: i32 },
    Swl { rs: usize, rt: usize, imm_sext: i32 },
    Sw { rs: usize, rt: usize, imm_sext: i32 },
    Swr { rs: usize, rt: usize, imm_sext: i32 },
}

impl Instruction {
    pub fn decode(ins: u32) -> Option<Self> {
        let rs = ((ins >> 21) & 0x1F) as usize;
        let rt = ((ins >> 16) & 0x1F) as usize;
        let rd = ((ins >> 11) & 0x1F) as usize;
        let shamt = (ins >> 6) & 0x1F;
        let imm = ins & 0xFFFF;
        let imm_sext = i32::from((imm as u16).cast_signed());
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
