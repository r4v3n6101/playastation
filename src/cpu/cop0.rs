/// Simplified Cop0 (coprocessor 0) with the logic used in PSX.
/// It's not fully MIPS-compatible, because PSX doesn't use TLB for example.
#[derive(Default, Debug)]
pub struct Cop0 {
    pub status: u32,
    pub state: u32,
    pub bad_vaddr: u32,
    pub epc: u32,
}
