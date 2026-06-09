#![no_std]

extern crate alloc;

pub mod cpu;
pub mod devices;
pub mod formats;
pub mod interconnect;
pub mod render;
pub mod run;

/// 2MiB of mapped RAM.
pub const RAM_SIZE: usize = 2 * 1024 * 1024;
/// 512KiB BIOS, ROM.
pub const BIOS_SIZE: usize = 512 * 1024;
