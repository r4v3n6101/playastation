use super::super::{Registers, ins::Opcode};

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

pub fn execute(ins: u32, op: Opcode, regs: &Registers) -> ExecRes {
    let rs = ((ins >> 21) & 0x1F) as usize;
    let rt = ((ins >> 16) & 0x1F) as usize;
    let rd = ((ins >> 11) & 0x1F) as usize;
    let shamt = (ins >> 6) & 0x1F;
    let imm = (ins & 0xFFFF) as u16;
    let imm_sext = imm.cast_signed();
    let target = ins & 0x03FF_FFFF;
    match op {
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
                let b = i32::from(imm_sext);
                a.checked_add(b).map(i32::cast_unsigned)
            },
        },
        Opcode::Addiu => ExecRes::Alu {
            dest: rt,
            res: Some(regs.general[rs].wrapping_add(imm_sext as u32)),
        },
        Opcode::Slti => ExecRes::Alu {
            dest: rt,
            res: Some((regs.general[rs].cast_signed() < i32::from(imm_sext)).into()),
        },
        Opcode::Sltiu => ExecRes::Alu {
            dest: rt,
            res: Some((regs.general[rs] < imm_sext as u32).into()),
        },
        Opcode::Andi => ExecRes::Alu {
            dest: rt,
            res: Some(regs.general[rs] & u32::from(imm)),
        },
        Opcode::Ori => ExecRes::Alu {
            dest: rt,
            res: Some(regs.general[rs] | u32::from(imm)),
        },
        Opcode::Xori => ExecRes::Alu {
            dest: rt,
            res: Some(regs.general[rs] ^ u32::from(imm)),
        },
        Opcode::Lui => ExecRes::Alu {
            dest: rt,
            res: Some(u32::from(imm) << 16),
        },
        Opcode::Lw => ExecRes::Load {
            dest: rt,
            addr: regs.general[rs].wrapping_add(imm_sext as u32),
            kind: LoadKind::Word,
        },
        Opcode::Lh => ExecRes::Load {
            dest: rt,
            addr: regs.general[rs].wrapping_add(imm_sext as u32),
            kind: LoadKind::IHalf,
        },
        Opcode::Lhu => ExecRes::Load {
            dest: rt,
            addr: regs.general[rs].wrapping_add(imm_sext as u32),
            kind: LoadKind::UHalf,
        },
        Opcode::Lb => ExecRes::Load {
            dest: rt,
            addr: regs.general[rs].wrapping_add(imm_sext as u32),
            kind: LoadKind::IByte,
        },
        Opcode::Lbu => ExecRes::Load {
            dest: rt,
            addr: regs.general[rs].wrapping_add(imm_sext as u32),
            kind: LoadKind::UByte,
        },
        Opcode::Lwl => ExecRes::Load {
            dest: rt,
            addr: regs.general[rs].wrapping_add(imm_sext as u32),
            kind: LoadKind::WordLeft,
        },
        Opcode::Lwr => ExecRes::Load {
            dest: rt,
            addr: regs.general[rs].wrapping_add(imm_sext as u32),
            kind: LoadKind::WordRight,
        },
        Opcode::Sw => ExecRes::Store {
            addr: regs.general[rs].wrapping_add(imm_sext as u32),
            kind: StoreKind::Word(regs.general[rt]),
        },
        Opcode::Sh => ExecRes::Store {
            addr: regs.general[rs].wrapping_add(imm_sext as u32),
            kind: StoreKind::Half(regs.general[rt] as u16),
        },
        Opcode::Sb => ExecRes::Store {
            addr: regs.general[rs].wrapping_add(imm_sext as u32),
            kind: StoreKind::Byte(regs.general[rt] as u8),
        },
        Opcode::Swl => ExecRes::Store {
            addr: regs.general[rs].wrapping_add(imm_sext as u32),
            kind: StoreKind::WordLeft(regs.general[rt]),
        },
        Opcode::Swr => ExecRes::Store {
            addr: regs.general[rs].wrapping_add(imm_sext as u32),
            kind: StoreKind::WordRight(regs.general[rt]),
        },
        Opcode::Beq => ExecRes::Branch {
            addr: (regs.general[rs] == regs.general[rt])
                .then_some((regs.pc - 4).wrapping_add((imm_sext << 2) as u32)),
            link: false,
        },
        Opcode::Bne => ExecRes::Branch {
            addr: (regs.general[rs] != regs.general[rt])
                .then_some((regs.pc - 4).wrapping_add((imm_sext << 2) as u32)),
            link: false,
        },
        Opcode::Blez => ExecRes::Branch {
            addr: (regs.general[rs].cast_signed() <= 0)
                .then_some((regs.pc - 4).wrapping_add((imm_sext << 2) as u32)),
            link: false,
        },
        Opcode::Bgtz => ExecRes::Branch {
            addr: (regs.general[rs].cast_signed() > 0)
                .then_some((regs.pc - 4).wrapping_add((imm_sext << 2) as u32)),
            link: false,
        },
        Opcode::Bltz => ExecRes::Branch {
            addr: (regs.general[rs].cast_signed() < 0)
                .then_some((regs.pc - 4).wrapping_add((imm_sext << 2) as u32)),
            link: false,
        },
        Opcode::Bgez => ExecRes::Branch {
            addr: (regs.general[rs].cast_signed() >= 0)
                .then_some((regs.pc - 4).wrapping_add((imm_sext << 2) as u32)),
            link: false,
        },
        Opcode::Bltzal => ExecRes::Branch {
            addr: (regs.general[rs].cast_signed() < 0)
                .then_some((regs.pc - 4).wrapping_add((imm_sext << 2) as u32)),
            link: true,
        },
        Opcode::Bgezal => ExecRes::Branch {
            addr: (regs.general[rs].cast_signed() >= 0)
                .then_some((regs.pc - 4).wrapping_add((imm_sext << 2) as u32)),
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
        Opcode::Mtlo => ExecRes::MulDiv {
            hi: regs.hi,
            lo: regs.general[rs],
        },
        Opcode::Mthi => ExecRes::MulDiv {
            hi: regs.general[rs],
            lo: regs.lo,
        },
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
        Opcode::Cfc0 => unimplemented!(),
        Opcode::Ctc0 => unimplemented!(),
        Opcode::Break => ExecRes::Break,
        Opcode::Syscall => ExecRes::Syscall,
        Opcode::Rfe => ExecRes::Rfe,
    }
}
