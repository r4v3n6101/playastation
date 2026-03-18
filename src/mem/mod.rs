const MEMSIZE: usize = 2 * 1024 * 1024;

pub struct Memory {
    pub storage: Box<[u8]>,
}

#[derive(Default)]
pub struct Bus {
    // TODO
    pub mem: Memory,
}

impl Default for Memory {
    fn default() -> Self {
        Self {
            storage: vec![0; MEMSIZE].into_boxed_slice(),
        }
    }
}

impl Bus {
    pub fn read_byte(&self, addr: u32) -> u8 {
        self.mem.storage[addr as usize]
    }

    pub fn read_half(&self, addr: u32) -> u16 {
        u16::from_le_bytes([
            self.mem.storage[addr as usize],
            self.mem.storage[(addr + 1) as usize],
        ])
    }

    pub fn read_word(&self, addr: u32) -> u32 {
        // TODO : check alignment
        u32::from_le_bytes([
            self.mem.storage[addr as usize],
            self.mem.storage[(addr + 1) as usize],
            self.mem.storage[(addr + 2) as usize],
            self.mem.storage[(addr + 3) as usize],
        ])
    }

    pub fn store_byte(&mut self, addr: u32, value: u8) {
        todo!()
    }

    pub fn store_half(&mut self, addr: u32, value: u16) {
        todo!()
    }

    pub fn store_word(&mut self, addr: u32, value: u32) {
        let [a, b, c, d] = value.to_le_bytes();
        self.mem.storage[addr as usize] = a;
        self.mem.storage[(addr + 1) as usize] = b;
        self.mem.storage[(addr + 2) as usize] = c;
        self.mem.storage[(addr + 3) as usize] = d;
    }
}
