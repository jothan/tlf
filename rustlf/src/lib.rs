// unsafe, unsafe everywhere
#![allow(clippy::missing_safety_doc)]

pub mod audio;
pub mod background_process;
pub mod bands;
mod cw_utils;
pub mod err_utils;
pub mod ffi;
pub mod fldigi;
mod foreground;
mod hamlib;
mod netkeyer;
pub mod newtlf;
mod qtcutil;
pub mod workqueue;
pub mod write_keyer;
