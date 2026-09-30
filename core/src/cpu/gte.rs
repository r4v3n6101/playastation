//! Geometry Transformation Engine (COP2).
//!
//! PSX-SPX's GTE specification is used:
//! https://psx-spx.consoledev.net/geometrytransformationenginegte/
//!
//! Commands complete synchronously; the interpreter accounts for their latency.

use core::array;

// The hardware divider uses a reciprocal table and two UNR refinement steps.
const UNR_TABLE: [u8; 257] = {
    let mut table = [0; 257];
    let mut i = 0;
    while i < table.len() {
        let value = (0x40000 / (i + 0x100)).div_ceil(2);
        table[i] = value.saturating_sub(0x101) as u8;
        i += 1;
    }
    table
};

#[derive(Debug, Clone, Copy, Default)]
pub struct Gte {
    pub data: [u32; 32],
    pub control: [u32; 32],
}

impl Gte {
    pub fn read_data(&self, reg: u8) -> u32 {
        match reg {
            15 => self.data[14], // SXYP mirrors the newest screen coordinate.
            28 | 29 => {
                let ir = self.ir();
                (0..3).fold(0, |rgb, i| {
                    rgb | ((ir[i] >> 7).clamp(0, 31) as i32).cast_unsigned() << (i * 5)
                })
            }
            _ => self.data[usize::from(reg)],
        }
    }

    pub fn write_data(&mut self, reg: u8, value: u32) {
        match reg {
            1 | 3 | 5 | 8..=11 => {
                self.data[usize::from(reg)] = i32::from(value as i16).cast_unsigned()
            }
            7 | 16..=19 => self.data[usize::from(reg)] = value & 0xffff,
            15 => self.push_sxy(value),
            28 => {
                for i in 0..3 {
                    self.data[9 + i] = ((value >> (5 * i)) & 31) << 7;
                }
            }
            29 | 31 => {} // ORGB and LZCR are read-only.
            30 => {
                self.data[30] = value;
                self.data[31] = if value.cast_signed() >= 0 {
                    value.leading_zeros()
                } else {
                    value.leading_ones()
                };
            }
            _ => self.data[usize::from(reg)] = value,
        }
    }

    pub fn read_control(&self, reg: u8) -> u32 {
        self.control[usize::from(reg)]
    }

    pub fn write_control(&mut self, reg: u8, value: u32) {
        self.control[usize::from(reg)] = match reg {
            // H is unsigned for projection, but sign-extended when read by CFC2.
            4 | 12 | 20 | 26 | 27 | 29 | 30 => i32::from(value as i16).cast_unsigned(),
            31 => value & 0x7fff_f000,
            _ => value,
        };
        if reg == 31 {
            self.update_error_flag();
        }
    }

    pub fn execute(&mut self, command: u32) -> u64 {
        let shift = if command & (1 << 19) != 0 { 12 } else { 0 };
        let lm = command & (1 << 10) != 0;
        self.control[31] = 0;

        let cycles = match command & 0x3f {
            0x01 => {
                self.project(0, shift, lm, true);
                15
            }
            0x06 => {
                let xy: [[i64; 2]; 3] = array::from_fn(|i| {
                    [
                        i64::from(self.data[12 + i] as i16),
                        i64::from((self.data[12 + i] >> 16) as i16),
                    ]
                });
                self.set_mac0(
                    xy[0][0] * (xy[1][1] - xy[2][1])
                        + xy[1][0] * (xy[2][1] - xy[0][1])
                        + xy[2][0] * (xy[0][1] - xy[1][1]),
                );
                8
            }
            0x0c => {
                let ir = self.ir();
                let d = [
                    i64::from(self.control[0] as i16),
                    i64::from(self.control[2] as i16),
                    i64::from(self.control[4] as i16),
                ];
                self.store_vector(
                    [
                        ir[2] * d[1] - ir[1] * d[2],
                        ir[0] * d[2] - ir[2] * d[0],
                        ir[1] * d[0] - ir[0] * d[1],
                    ],
                    shift,
                    lm,
                );
                6
            }
            0x10 => {
                self.depth_cue(self.color(self.data[6]).map(|c| c << 16), shift, lm);
                self.push_rgb();
                8
            }
            0x11 => {
                self.depth_cue(self.ir().map(|v| v << 12), shift, lm);
                self.push_rgb();
                8
            }
            0x12 => {
                self.mvmva(command, shift, lm);
                8
            }
            0x13 | 0x16 | 0x1b | 0x1e | 0x20 | 0x3f => {
                let opcode = command & 0x3f;
                let count = if matches!(opcode, 0x16 | 0x20 | 0x3f) {
                    3
                } else {
                    1
                };
                for i in 0..count {
                    self.multiply(self.matrix(8), self.vector(i), [0; 3], shift);
                    self.update_ir(lm);
                    self.color_matrix(shift, lm);
                    if matches!(opcode, 0x13 | 0x16) {
                        self.depth_cue(self.modulated_color(), shift, lm);
                    } else if matches!(opcode, 0x1b | 0x3f) {
                        self.store_vector(self.modulated_color(), shift, lm);
                    }
                    self.push_rgb();
                }
                match opcode {
                    0x13 => 19,
                    0x16 => 44,
                    0x1b => 17,
                    0x1e => 14,
                    0x20 => 30,
                    _ => 39,
                }
            }
            0x14 | 0x1c => {
                self.color_matrix(shift, lm);
                if command & 0x3f == 0x14 {
                    self.depth_cue(self.modulated_color(), shift, lm);
                } else {
                    self.store_vector(self.modulated_color(), shift, lm);
                }
                self.push_rgb();
                if command & 0x3f == 0x14 { 13 } else { 11 }
            }
            0x28 => {
                self.store_vector(self.ir().map(|v| v * v), shift, lm);
                5
            }
            0x29 => {
                self.depth_cue(self.modulated_color(), shift, lm);
                self.push_rgb();
                8
            }
            0x2a => {
                for _ in 0..3 {
                    self.depth_cue(self.color(self.data[20]).map(|c| c << 16), shift, lm);
                    self.push_rgb();
                }
                17
            }
            0x2d | 0x2e => {
                let four = command & 0x3f == 0x2e;
                let start = if four { 16 } else { 17 };
                let sum: i64 = self.data[start..20].iter().map(|&v| i64::from(v)).sum();
                let value = sum * i64::from(self.control[if four { 30 } else { 29 }].cast_signed());
                self.set_mac0(value);
                self.data[7] = (self.saturate(value >> 12, 0, 0xffff, 18) as i32).cast_unsigned();
                if four { 6 } else { 5 }
            }
            0x30 => {
                for i in 0..3 {
                    self.project(i, shift, lm, i == 2);
                }
                23
            }
            0x3d | 0x3e => {
                let ir = self.ir();
                let values = array::from_fn(|i| {
                    let base = if command & 0x3f == 0x3e {
                        i64::from(self.data[25 + i].cast_signed()) << shift
                    } else {
                        0
                    };
                    base + ir[i] * i64::from(self.data[8].cast_signed())
                });
                self.store_vector(values, shift, lm);
                self.push_rgb();
                5
            }
            _ => {
                tracing::warn!(command, "unknown GTE command");
                1
            }
        };
        self.update_error_flag();
        cycles
    }

    fn project(&mut self, vertex: usize, shift: u32, lm: bool, last: bool) {
        let values = self.multiply(
            self.matrix(0),
            self.vector(vertex),
            self.translation(5),
            shift,
        );
        for i in 0..2 {
            self.data[9 + i] = (self.saturate(
                i64::from(self.data[25 + i].cast_signed()),
                if lm { 0 } else { -0x8000 },
                0x7fff,
                24 - i,
            ) as i32)
                .cast_unsigned();
        }
        self.saturate(values[2] >> 12, -0x8000, 0x7fff, 22);
        self.data[11] = self.data[27]
            .cast_signed()
            .clamp(if lm { 0 } else { -0x8000 }, 0x7fff)
            .cast_unsigned();
        self.data[16] = self.data[17];
        self.data[17] = self.data[18];
        self.data[18] = self.data[19];
        self.data[19] = (self.saturate(values[2] >> 12, 0, 0xffff, 18) as i32).cast_unsigned();

        let factor = i64::from(self.divide(self.control[26] as u16, self.data[19] as u16));
        let mut xy = [0; 2];
        for (i, coord) in xy.iter_mut().enumerate() {
            let value = factor * i64::from(self.data[9 + i].cast_signed())
                + i64::from(self.control[24 + i].cast_signed());
            self.check_mac0(value);
            *coord = u32::from(
                (self.saturate(value >> 16, -0x400, 0x3ff, 14 - i) as i16).cast_unsigned(),
            );
        }
        self.push_sxy(xy[0] | (xy[1] << 16));
        if last {
            let depth = factor * i64::from(self.control[27].cast_signed())
                + i64::from(self.control[28].cast_signed());
            self.set_mac0(depth);
            self.data[8] = (self.saturate(depth >> 12, 0, 0x1000, 12) as i32).cast_unsigned();
        }
    }

    fn mvmva(&mut self, command: u32, shift: u32, lm: bool) {
        let matrix = match (command >> 17) & 3 {
            0 => self.matrix(0),
            1 => self.matrix(8),
            2 => self.matrix(16),
            _ => {
                let red = i64::from((self.data[6] & 0xff) << 4);
                [
                    [-red, red, i64::from(self.data[8].cast_signed())],
                    [i64::from(self.control[1] as i16); 3],
                    [i64::from(self.control[2] as i16); 3],
                ]
            }
        };
        let vector = self.vector(usize::from(((command >> 15) & 3) as u8));
        match (command >> 13) & 3 {
            0 => {
                self.multiply(matrix, vector, self.translation(5), shift);
            }
            1 => {
                self.multiply(matrix, vector, self.translation(13), shift);
            }
            2 => {
                for (i, row) in matrix.iter().enumerate() {
                    let discarded = self.wrap_mac(
                        i + 1,
                        i64::from(self.control[21 + i].cast_signed()) * 0x1000 + row[0] * vector[0],
                    );
                    self.saturate(
                        i64::from((discarded >> shift) as i32),
                        -0x8000,
                        0x7fff,
                        24 - i,
                    );
                    self.data[25 + i] = ((self
                        .wrap_mac(i + 1, row[1] * vector[1] + row[2] * vector[2])
                        >> shift) as i32)
                        .cast_unsigned();
                }
            }
            _ => {
                self.multiply(matrix, vector, [0; 3], shift);
            }
        }
        self.update_ir(lm);
    }

    fn color_matrix(&mut self, shift: u32, lm: bool) {
        self.multiply(self.matrix(16), self.ir(), self.translation(13), shift);
        self.update_ir(lm);
    }

    fn depth_cue(&mut self, base: [i64; 3], shift: u32, lm: bool) {
        let mut values = [0; 3];
        for i in 0..3 {
            let delta = self.wrap_mac(
                i + 1,
                i64::from(self.control[21 + i].cast_signed()) * 0x1000 - base[i],
            );
            let ir = self.saturate(i64::from((delta >> shift) as i32), -0x8000, 0x7fff, 24 - i);
            values[i] = base[i] + ir * i64::from(self.data[8].cast_signed());
        }
        self.store_vector(values, shift, lm);
    }

    fn modulated_color(&self) -> [i64; 3] {
        let rgb = self.color(self.data[6]);
        let ir = self.ir();
        array::from_fn(|i| (rgb[i] * ir[i]) << 4)
    }

    fn store_vector(&mut self, values: [i64; 3], shift: u32, lm: bool) {
        for (i, value) in values.into_iter().enumerate() {
            self.data[25 + i] = ((self.wrap_mac(i + 1, value) >> shift) as i32).cast_unsigned();
        }
        self.update_ir(lm);
    }

    fn multiply(
        &mut self,
        matrix: [[i64; 3]; 3],
        vector: [i64; 3],
        translation: [i64; 3],
        shift: u32,
    ) -> [i64; 3] {
        array::from_fn(|i| {
            let mut value = translation[i] << 12;
            for (j, &v) in vector.iter().enumerate() {
                value = self.wrap_mac(i + 1, value + matrix[i][j] * v);
            }
            self.data[25 + i] = ((value >> shift) as i32).cast_unsigned();
            value
        })
    }

    fn push_rgb(&mut self) {
        let mut rgb = self.data[6] & 0xff00_0000;
        for i in 0..3 {
            let value = self.saturate(
                i64::from(self.data[25 + i].cast_signed()) >> 4,
                0,
                255,
                21 - i,
            );
            rgb |= (value as i32).cast_unsigned() << (8 * i);
        }
        self.data[20] = self.data[21];
        self.data[21] = self.data[22];
        self.data[22] = rgb;
    }

    fn update_ir(&mut self, lm: bool) {
        for n in 1..=3 {
            self.data[8 + n] = (self.saturate(
                i64::from(self.data[24 + n].cast_signed()),
                if lm { 0 } else { -0x8000 },
                0x7fff,
                25 - n,
            ) as i32)
                .cast_unsigned();
        }
    }

    fn set_mac0(&mut self, value: i64) {
        self.check_mac0(value);
        self.data[24] = (value as i32).cast_unsigned();
    }

    fn vector(&self, index: usize) -> [i64; 3] {
        if index == 3 {
            self.ir()
        } else {
            [
                i64::from(self.data[index * 2] as i16),
                i64::from((self.data[index * 2] >> 16) as i16),
                i64::from(self.data[index * 2 + 1] as i16),
            ]
        }
    }

    fn divide(&mut self, h: u16, sz: u16) -> u32 {
        if u32::from(h) >= u32::from(sz) * 2 {
            self.control[31] |= 1 << 17;
            return 0x1ffff;
        }
        let shift = sz.leading_zeros();
        let numerator = u64::from(h) << shift;
        let divisor = u32::from(sz) << shift;
        let index = usize::from(((divisor - 0x7fc0) >> 7) as u16);
        let reciprocal = u32::from(UNR_TABLE[index]) + 0x101;
        let estimate = (0x2000080 - divisor * reciprocal) >> 8;
        let refined = (0x80 + estimate * reciprocal) >> 8;
        (((numerator * u64::from(refined) + 0x8000) >> 16) as u32).min(0x1ffff)
    }

    fn matrix(&self, base: usize) -> [[i64; 3]; 3] {
        array::from_fn(|row| {
            array::from_fn(|col| {
                let i = row * 3 + col;
                i64::from((self.control[base + i / 2] >> ((i & 1) * 16)) as i16)
            })
        })
    }

    fn translation(&self, base: usize) -> [i64; 3] {
        array::from_fn(|i| i64::from(self.control[base + i].cast_signed()))
    }

    fn ir(&self) -> [i64; 3] {
        array::from_fn(|i| i64::from(self.data[9 + i].cast_signed()))
    }

    fn color(&self, rgb: u32) -> [i64; 3] {
        array::from_fn(|i| i64::from((rgb >> (i * 8)) & 0xff))
    }

    fn push_sxy(&mut self, value: u32) {
        self.data[12] = self.data[13];
        self.data[13] = self.data[14];
        self.data[14] = value;
    }

    fn wrap_mac(&mut self, n: usize, value: i64) -> i64 {
        if value > (1i64 << 43) - 1 {
            self.control[31] |= 1 << (31 - n);
        }
        if value < -(1i64 << 43) {
            self.control[31] |= 1 << (28 - n);
        }
        (value << 20) >> 20
    }

    fn check_mac0(&mut self, value: i64) {
        if value > i64::from(i32::MAX) {
            self.control[31] |= 1 << 16;
        }
        if value < i64::from(i32::MIN) {
            self.control[31] |= 1 << 15;
        }
    }

    fn saturate(&mut self, value: i64, min: i64, max: i64, flag: usize) -> i64 {
        if value < min || value > max {
            self.control[31] |= 1 << flag;
        }
        value.clamp(min, max)
    }

    fn update_error_flag(&mut self) {
        self.control[31] &= 0x7fff_ffff;
        if self.control[31] & 0x7f87_e000 != 0 {
            self.control[31] |= 1 << 31;
        }
    }
}
