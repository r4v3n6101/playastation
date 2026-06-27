#![no_std]

extern crate alloc;

pub mod cpu;
pub mod devices;
pub mod formats;
pub mod interconnect;
pub mod render;
pub mod run;

/// CPU Frequency (ticks per second).
pub const CPU_FREQ: u64 = 33_868_800;

/// 2MiB of mapped RAM.
pub const RAM_SIZE: usize = 2 * 1024 * 1024;
/// 512KiB BIOS, ROM.
pub const BIOS_SIZE: usize = 512 * 1024;

/// Width of VRAM buffer.
pub const VRAM_WIDTH: usize = 1024;
/// Height of VRAM buffer.
pub const VRAM_HEIGHT: usize = 512;
