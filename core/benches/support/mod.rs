use playastation::{
    cpu::{Cpu, Exception},
    interconnect::{RAM_SIZE, bus::Bus},
    run::{CpuEngine, ExecutionMode, ExecutionResult, StopReason},
};

const CODE: usize = 0x40000;
const START: u32 = 0x8000_0000 | CODE as u32;
const INPUT: usize = 0x100000;
const OUTPUT: usize = 0x110000;
const ROUNDS: u32 = 4096;
const BREAK: u32 = 0x0000_000d;

pub(crate) struct Workload {
    pub(crate) name: &'static str,
    pub(crate) cpu: Cpu,
    ram: Vec<u8>,
    expected_cpu: Cpu,
    expected_ram: Vec<u8>,
    pub(crate) instructions: u64,
    pub(crate) cycles: u64,
}

impl Workload {
    fn new(name: &'static str, code: &[u32], instructions: u64, cycles: u64) -> Self {
        assert_eq!(code.last(), Some(&BREAK));
        let cpu = Cpu {
            pc: START,
            ..Cpu::default()
        };
        let mut ram = vec![0; RAM_SIZE];
        for (i, word) in code.iter().enumerate() {
            ram[CODE + i * 4..CODE + i * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        let expected_cpu = Cpu {
            pc: START + (code.len() as u32 - 1) * 4,
            ..cpu
        };
        Self {
            name,
            cpu,
            expected_cpu,
            expected_ram: ram.clone(),
            ram,
            instructions,
            cycles,
        }
    }

    pub(crate) fn bus(&self) -> Bus {
        let mut bus = Bus::default();
        bus.ram.copy_from_slice(&self.ram);
        bus
    }

    pub(crate) fn check(&self, cpu: &Cpu, bus: &Bus, result: &ExecutionResult) {
        assert!(matches!(
            result.stop_reason,
            StopReason::Exception(Exception::Break)
        ));
        assert_eq!(result.cycles_elapsed, self.cycles, "{} cycles", self.name);
        assert_eq!(cpu.pc, self.expected_cpu.pc, "{} PC", self.name);
        assert_eq!(cpu.gpr, self.expected_cpu.gpr, "{} registers", self.name);
        assert_eq!((cpu.hi, cpu.lo), (0, 0));
        assert!(cpu.pending_jump.is_none());
        assert_eq!((cpu.pending_load.dest, cpu.pending_load.value), (0, 0));
        assert!(bus.ram.as_ref() == self.expected_ram, "{} RAM", self.name);
    }

    pub(crate) fn validate(&self, mode: ExecutionMode) {
        let mut engine = CpuEngine::new(mode);
        let mut bus = self.bus();
        // Check first execution, reuse of the same cache, and execution after a clear.
        for clear in [false, false, true] {
            if clear {
                engine.cache_invalidate_all();
            }
            let mut cpu = self.cpu;
            let result = engine.run_for(&mut cpu, &mut bus, self.cycles);
            self.check(&cpu, &bus, &result);
        }
    }
}

fn alu() -> Workload {
    let code = [
        0x0008_1340, // sll   v0,t0,13
        0x0102_4026, // xor   t0,t0,v0
        0x0008_1442, // srl   v0,t0,17
        0x0102_4026, // xor   t0,t0,v0
        0x0008_1140, // sll   v0,t0,5
        0x0102_4026, // xor   t0,t0,v0
        0x0128_4821, // addu  t1,t1,t0
        0x2484_ffff, // addiu a0,a0,-1
        0x1480_fff7, // bne   a0,zero,start
        0,           // nop (delay slot)
        BREAK,
    ];
    let count = u64::from(ROUNDS) * 10 + 1;
    let mut w = Workload::new("alu", &code, count, count);
    w.cpu.gpr[4] = ROUNDS;
    w.cpu.gpr[8] = 0x1234_5678;
    let mut x = w.cpu.gpr[8];
    let mut sum = 0u32;
    for _ in 0..ROUNDS {
        x ^= x << 13;
        x ^= x >> 17;
        w.expected_cpu.gpr[2] = x << 5;
        x ^= x << 5;
        sum = sum.wrapping_add(x);
    }
    w.expected_cpu.gpr[8] = x;
    w.expected_cpu.gpr[9] = sum;
    w
}

fn branches() -> Workload {
    let join = (START + 9 * 4) >> 2 & 0x03ff_ffff;
    let code = [
        0x3102_0001,        // andi  v0,t0,1
        0x1040_0004,        // beq   v0,zero,even
        0x0008_4042,        // srl   t0,t0,1 (delay slot)
        0x2529_0003,        // addiu t1,t1,3
        0x0800_0000 | join, // j     join
        0,                  // nop
        0x2529_0007,        // even: addiu t1,t1,7
        0x0800_0000 | join, // j     join
        0,                  // nop
        0x0109_4026,        // join: xor t0,t0,t1
        0x2484_ffff,        // addiu a0,a0,-1
        0x1480_fff4,        // bne   a0,zero,start
        0,                  // nop
        BREAK,
    ];
    let count = u64::from(ROUNDS) * 10 + 1;
    let mut w = Workload::new("branches", &code, count, count);
    w.cpu.gpr[4] = ROUNDS;
    w.cpu.gpr[8] = 0x1234_5678;
    let mut x = w.cpu.gpr[8];
    let mut sum = 0u32;
    for _ in 0..ROUNDS {
        w.expected_cpu.gpr[2] = x & 1;
        sum = sum.wrapping_add(if x & 1 == 0 { 7 } else { 3 });
        x = (x >> 1) ^ sum;
    }
    w.expected_cpu.gpr[8] = x;
    w.expected_cpu.gpr[9] = sum;
    w
}

fn ram_copy() -> Workload {
    let code = [
        0x8ca8_0000, // lw    t0,0(a1)
        0x24a5_0004, // addiu a1,a1,4 (load delay)
        0x0128_4821, // addu  t1,t1,t0
        0xacc8_0000, // sw    t0,0(a2)
        0x24c6_0004, // addiu a2,a2,4
        0x2484_ffff, // addiu a0,a0,-1
        0x1480_fff9, // bne   a0,zero,start
        0,           // nop
        BREAK,
    ];
    let n = u64::from(ROUNDS);
    let mut w = Workload::new("ram_copy", &code, n * 8 + 1, n * 13 + 1);
    w.cpu.gpr[4] = ROUNDS;
    w.cpu.gpr[5] = 0x8000_0000 | INPUT as u32;
    w.cpu.gpr[6] = 0x8000_0000 | OUTPUT as u32;
    let mut sum = 0u32;
    for i in 0..ROUNDS as usize {
        let value = (i as u32).wrapping_mul(0x9e37_79b9).rotate_left(7);
        let bytes = value.to_le_bytes();
        w.ram[INPUT + i * 4..INPUT + i * 4 + 4].copy_from_slice(&bytes);
        w.expected_ram[INPUT + i * 4..INPUT + i * 4 + 4].copy_from_slice(&bytes);
        w.expected_ram[OUTPUT + i * 4..OUTPUT + i * 4 + 4].copy_from_slice(&bytes);
        sum = sum.wrapping_add(value);
        w.expected_cpu.gpr[8] = value;
    }
    w.expected_cpu.gpr[5] = w.cpu.gpr[5] + ROUNDS * 4;
    w.expected_cpu.gpr[6] = w.cpu.gpr[6] + ROUNDS * 4;
    w.expected_cpu.gpr[9] = sum;
    w
}

fn dispatch(name: &'static str, blocks: usize) -> Workload {
    assert!(blocks.is_power_of_two());
    // Eight instructions per block, spaced 64 bytes apart. The odd multiplier
    // visits every block once in a non-sequential order before reaching BREAK.
    let mut code = vec![0; blocks * 16 + 1];
    for visit in 0..blocks {
        let index = (visit * 509) % blocks;
        let next = if visit + 1 == blocks {
            blocks
        } else {
            ((visit + 1) * 509) % blocks
        };
        let target = (START + next as u32 * 64) >> 2 & 0x03ff_ffff;
        code[index * 16..index * 16 + 8].copy_from_slice(&[
            0x2508_0001,          // addiu t0,t0,1
            0x0128_4826,          // xor   t1,t1,t0
            0x0149_5021,          // addu  t2,t2,t1
            0x314b_00ff,          // andi  t3,t2,255
            0x000b_6040,          // sll   t4,t3,1
            0x0188_6825,          // or    t5,t4,t0
            0x0800_0000 | target, // j     next
            0,                    // nop
        ]);
    }
    *code.last_mut().unwrap() = BREAK;
    let count = blocks as u64 * 8 + 1;
    let mut w = Workload::new(name, &code, count, count);
    let r = &mut w.expected_cpu.gpr;
    for _ in 0..blocks {
        r[8] += 1;
        r[9] ^= r[8];
        r[10] = r[10].wrapping_add(r[9]);
        r[11] = r[10] & 255;
        r[12] = r[11] << 1;
        r[13] = r[12] | r[8];
    }
    w
}

fn invalidation() -> Workload {
    let code = [
        0xaca9_0004, // sw    t1,4(a1): overwrite the next instruction
        0x2508_0001, // addiu t0,t0,1
        0x2484_ffff, // addiu a0,a0,-1
        0x1480_fffc, // bne   a0,zero,start
        0,           // nop
        BREAK,
    ];
    let rounds = 256;
    let mut w = Workload::new("invalidation", &code, rounds * 5 + 1, rounds * 7 + 1);
    w.cpu.gpr[4] = rounds as u32;
    w.cpu.gpr[5] = START;
    w.cpu.gpr[9] = code[1];
    w.expected_cpu.gpr[5] = START;
    w.expected_cpu.gpr[8] = rounds as u32;
    w.expected_cpu.gpr[9] = code[1];
    w
}

pub(crate) fn workloads() -> [Workload; 6] {
    [
        alu(),
        branches(),
        ram_copy(),
        dispatch("dispatch_64", 64),
        dispatch("dispatch_4096", 4096),
        invalidation(),
    ]
}
