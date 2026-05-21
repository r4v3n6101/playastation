// Used for software renderer multi-threading
#![cfg_attr(not(feature = "software-renderer"), no_std)]

extern crate alloc;

pub mod cpu;
pub mod devices;
pub mod formats;
pub mod globals;
pub mod interconnect;
pub mod render;
pub mod run;
