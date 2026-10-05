#![no_std]

extern crate alloc;

pub mod cpu;
pub mod devices;
pub mod formats;
pub mod interconnect;
pub mod render;
pub mod run;

/// This is CPU time in cycles.
pub type SystemTime = u64;
