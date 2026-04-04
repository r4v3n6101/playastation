use strum::FromRepr;

use super::Registers;

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

#[derive(Debug, Copy, Clone)]
pub enum ExecRes {
    /// Arithmetic ops like ADD, SUB, OR, etc. and shifts too
    /// [`res`] may be [`Option::None`] if overflow happened.
    Alu { dest: usize, res: Option<u32> },
    /// Load from memory, store into register.
    Load {
        dest: usize,
        addr: u32,
        kind: LoadKind,
    },
    /// Store in memory value from instruction.
    Store { addr: u32, kind: StoreKind },
    /// Conditions.
    /// [`addr`] is [`Option::None`] when comparison failed.
    Branch { addr: Option<u32>, link: bool },
    /// Jump!
    Jump {
        addr: u32,
        link: bool,
        link_reg: usize,
    },
    /// Multiple and divide (uses extra registers HI/LO).
    MulDiv { hi: u32, lo: u32 },
    /// Move from coprocessor 0.
    Mfc0 { dest: usize, from: usize },
    /// Move to coprocessor 0.
    Mtc0 { dest: usize, res: u32 },
    /// Break.
    Break,
    /// Syscall.
    Syscall,
    /// Return from exception.
    Rfe,
}

#[derive(Debug, Copy, Clone)]
pub enum LoadKind {
    /// Byte (signed).
    IByte,
    /// Halfword (signed).
    IHalf,
    /// Byte (unsigned).
    UByte,
    /// Halfword (unsigned).
    UHalf,
    /// Word.
    Word,
    /// Word left.
    WordLeft,
    /// Word right.
    WordRight,
}

#[derive(Debug, Copy, Clone)]
pub enum StoreKind {
    /// Byte.
    Byte(u8),
    /// Halfword.
    Half(u16),
    /// Word.
    Word(u32),
    /// Word left.
    WordLeft(u32),
    /// Word right.
    WordRight(u32),
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

    pub fn execute(self, ins: u32, regs: &Registers) -> ExecRes {
        let rs = ((ins >> 21) & 0x1F) as usize;
        let rt = ((ins >> 16) & 0x1F) as usize;
        let rd = ((ins >> 11) & 0x1F) as usize;
        let shamt = (ins >> 6) & 0x1F;
        let imm = ins & 0xFFFF;
        let imm_sext = (imm as u16).cast_signed() as u32;
        let target = ins & 0x03FF_FFFF;
        match self {
            Opcode::Add => ExecRes::Alu {
                dest: rd,
                res: {
                    regs.general[rs]
                        .cast_signed()
                        .checked_add(regs.general[rt].cast_signed())
                        .map(i32::cast_unsigned)
                },
            },
            Opcode::Addu => ExecRes::Alu {
                dest: rd,
                res: Some(regs.general[rs].wrapping_add(regs.general[rt])),
            },
            Opcode::Sub => ExecRes::Alu {
                dest: rd,
                res: {
                    regs.general[rs]
                        .cast_signed()
                        .checked_sub(regs.general[rt].cast_signed())
                        .map(i32::cast_unsigned)
                },
            },
            Opcode::Subu => ExecRes::Alu {
                dest: rd,
                res: Some(regs.general[rs].wrapping_sub(regs.general[rt])),
            },
            Opcode::And => ExecRes::Alu {
                dest: rd,
                res: Some(regs.general[rs] & regs.general[rt]),
            },
            Opcode::Or => ExecRes::Alu {
                dest: rd,
                res: Some(regs.general[rs] | regs.general[rt]),
            },
            Opcode::Xor => ExecRes::Alu {
                dest: rd,
                res: Some(regs.general[rs] ^ regs.general[rt]),
            },
            Opcode::Nor => ExecRes::Alu {
                dest: rd,
                res: Some(!(regs.general[rs] | regs.general[rt])),
            },
            Opcode::Slt => ExecRes::Alu {
                dest: rd,
                res: Some((regs.general[rs].cast_signed() < regs.general[rt].cast_signed()).into()),
            },
            Opcode::Sltu => ExecRes::Alu {
                dest: rd,
                res: Some((regs.general[rs] < regs.general[rt]).into()),
            },
            Opcode::Sll => ExecRes::Alu {
                dest: rd,
                res: Some(regs.general[rt] << shamt),
            },
            Opcode::Srl => ExecRes::Alu {
                dest: rd,
                res: Some(regs.general[rt] >> shamt),
            },
            Opcode::Sra => ExecRes::Alu {
                dest: rd,
                res: Some((regs.general[rt].cast_signed() >> shamt).cast_unsigned()),
            },
            Opcode::Sllv => ExecRes::Alu {
                dest: rd,
                res: Some(regs.general[rt] << (regs.general[rs] & 0x1F)),
            },
            Opcode::Srlv => ExecRes::Alu {
                dest: rd,
                res: Some(regs.general[rt] >> (regs.general[rs] & 0x1F)),
            },
            Opcode::Srav => ExecRes::Alu {
                dest: rd,
                res: Some(
                    (regs.general[rt].cast_signed() >> (regs.general[rs] & 0x1F)).cast_unsigned(),
                ),
            },
            Opcode::Addi => ExecRes::Alu {
                dest: rt,
                res: {
                    let a = regs.general[rs].cast_signed();
                    let b = imm_sext as i16 as i32;
                    a.checked_add(b).map(i32::cast_unsigned)
                },
            },
            Opcode::Addiu => ExecRes::Alu {
                dest: rt,
                res: Some(regs.general[rs].wrapping_add(imm_sext)),
            },
            Opcode::Slti => ExecRes::Alu {
                dest: rt,
                res: Some((regs.general[rs].cast_signed() < imm_sext.cast_signed()).into()),
            },
            Opcode::Sltiu => ExecRes::Alu {
                dest: rt,
                res: Some((regs.general[rs] < imm_sext).into()),
            },
            Opcode::Andi => ExecRes::Alu {
                dest: rt,
                res: Some(regs.general[rs] & imm),
            },
            Opcode::Ori => ExecRes::Alu {
                dest: rt,
                res: Some(regs.general[rs] | imm),
            },
            Opcode::Xori => ExecRes::Alu {
                dest: rt,
                res: Some(regs.general[rs] ^ imm),
            },
            Opcode::Lui => ExecRes::Alu {
                dest: rt,
                res: Some(imm << 16),
            },
            Opcode::Lw => ExecRes::Load {
                dest: rt,
                addr: regs.general[rs].wrapping_add(imm_sext),
                kind: LoadKind::Word,
            },
            Opcode::Lh => ExecRes::Load {
                dest: rt,
                addr: regs.general[rs].wrapping_add(imm_sext),
                kind: LoadKind::IHalf,
            },
            Opcode::Lhu => ExecRes::Load {
                dest: rt,
                addr: regs.general[rs].wrapping_add(imm_sext),
                kind: LoadKind::UHalf,
            },
            Opcode::Lb => ExecRes::Load {
                dest: rt,
                addr: regs.general[rs].wrapping_add(imm_sext),
                kind: LoadKind::IByte,
            },
            Opcode::Lbu => ExecRes::Load {
                dest: rt,
                addr: regs.general[rs].wrapping_add(imm_sext),
                kind: LoadKind::UByte,
            },
            Opcode::Lwl => ExecRes::Load {
                dest: rt,
                addr: regs.general[rs].wrapping_add(imm_sext),
                kind: LoadKind::WordLeft,
            },
            Opcode::Lwr => ExecRes::Load {
                dest: rt,
                addr: regs.general[rs].wrapping_add(imm_sext),
                kind: LoadKind::WordRight,
            },
            Opcode::Sw => ExecRes::Store {
                addr: regs.general[rs].wrapping_add(imm_sext),
                kind: StoreKind::Word(regs.general[rt]),
            },
            Opcode::Sh => ExecRes::Store {
                addr: regs.general[rs].wrapping_add(imm_sext),
                kind: StoreKind::Half(regs.general[rt] as u16),
            },
            Opcode::Sb => ExecRes::Store {
                addr: regs.general[rs].wrapping_add(imm_sext),
                kind: StoreKind::Byte(regs.general[rt] as u8),
            },
            Opcode::Swl => ExecRes::Store {
                addr: regs.general[rs].wrapping_add(imm_sext),
                kind: StoreKind::WordLeft(regs.general[rt]),
            },
            Opcode::Swr => ExecRes::Store {
                addr: regs.general[rs].wrapping_add(imm_sext),
                kind: StoreKind::WordRight(regs.general[rt]),
            },
            Opcode::Beq => ExecRes::Branch {
                addr: (regs.general[rs] == regs.general[rt])
                    .then_some((regs.pc - 4).wrapping_add(imm_sext << 2)),
                link: false,
            },
            Opcode::Bne => ExecRes::Branch {
                addr: (regs.general[rs] != regs.general[rt])
                    .then_some((regs.pc - 4).wrapping_add(imm_sext << 2)),
                link: false,
            },
            Opcode::Blez => ExecRes::Branch {
                addr: (regs.general[rs].cast_signed() <= 0)
                    .then_some((regs.pc - 4).wrapping_add(imm_sext << 2)),
                link: false,
            },
            Opcode::Bgtz => ExecRes::Branch {
                addr: (regs.general[rs].cast_signed() > 0)
                    .then_some((regs.pc - 4).wrapping_add(imm_sext << 2)),
                link: false,
            },
            Opcode::Bltz => ExecRes::Branch {
                addr: (regs.general[rs].cast_signed() < 0)
                    .then_some((regs.pc - 4).wrapping_add(imm_sext << 2)),
                link: false,
            },
            Opcode::Bgez => ExecRes::Branch {
                addr: (regs.general[rs].cast_signed() >= 0)
                    .then_some((regs.pc - 4).wrapping_add(imm_sext << 2)),
                link: false,
            },
            Opcode::Bltzal => ExecRes::Branch {
                addr: (regs.general[rs].cast_signed() < 0)
                    .then_some((regs.pc - 4).wrapping_add(imm_sext << 2)),
                link: true,
            },
            Opcode::Bgezal => ExecRes::Branch {
                addr: (regs.general[rs].cast_signed() >= 0)
                    .then_some((regs.pc - 4).wrapping_add(imm_sext << 2)),
                link: true,
            },
            Opcode::J => ExecRes::Jump {
                addr: ((regs.pc - 4) & 0xF000_0000) | (target << 2),
                link: false,
                link_reg: 31,
            },
            Opcode::Jal => ExecRes::Jump {
                addr: ((regs.pc - 4) & 0xF000_0000) | (target << 2),
                link: true,
                link_reg: 31,
            },
            Opcode::Mfhi => ExecRes::Alu {
                dest: rd,
                res: Some(regs.hi),
            },
            Opcode::Mflo => ExecRes::Alu {
                dest: rd,
                res: Some(regs.lo),
            },
            Opcode::Mult => {
                let a = i64::from(regs.general[rs].cast_signed());
                let b = i64::from(regs.general[rt].cast_signed());
                let res = (a * b).cast_unsigned();
                ExecRes::MulDiv {
                    hi: (res >> 32) as u32,
                    lo: res as u32,
                }
            }
            Opcode::Multu => {
                let a = u64::from(regs.general[rs]);
                let b = u64::from(regs.general[rt]);
                let res = a * b;
                ExecRes::MulDiv {
                    hi: (res >> 32) as u32,
                    lo: res as u32,
                }
            }
            Opcode::Div => {
                let a = regs.general[rs].cast_signed();
                let b = regs.general[rt].cast_signed();
                // Overflow or div by 0
                let (hi, lo) = if (b == 0) || (a.cast_unsigned() == 0x8000_0000 && b == -1) {
                    (a.cast_unsigned(), b.cast_unsigned())
                } else {
                    ((a % b).cast_unsigned(), (a / b).cast_unsigned())
                };
                ExecRes::MulDiv { hi, lo }
            }
            Opcode::Divu => {
                let a = regs.general[rs];
                let b = regs.general[rt];
                let (hi, lo) = if b == 0 { (a, b) } else { (a % b, a / b) };
                ExecRes::MulDiv { hi, lo }
            }
            Opcode::Jr => ExecRes::Jump {
                addr: regs.general[rs],
                link: false,
                link_reg: 31,
            },
            Opcode::Jalr => ExecRes::Jump {
                addr: regs.general[rs],
                link: true,
                link_reg: rd,
            },
            Opcode::Mfc0 => ExecRes::Mfc0 { dest: rt, from: rd },
            Opcode::Mtc0 => ExecRes::Mtc0 {
                dest: rd,
                res: regs.general[rt],
            },
            Opcode::Break => ExecRes::Break,
            Opcode::Syscall => ExecRes::Syscall,
            Opcode::Rfe => ExecRes::Rfe,
            other => {
                println!("{other:?}");
                unimplemented!()
            }
        }
    }

    pub fn branch_delay(self) -> bool {
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
