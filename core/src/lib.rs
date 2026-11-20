#![no_std]

extern crate alloc;

pub mod formats;
pub mod interfaces;
pub mod machine;
pub mod run;

/// This is CPU time in cycles.
pub type SystemTime = u64;
