// unsafe, unsafe everywhere
#![allow(clippy::missing_safety_doc)]
#![feature(linkage)]

pub mod audio;
pub mod background_process;
pub mod bands;
pub mod cqww_simulator;
mod cw_utils;
pub mod err_utils;
pub mod fldigi;
mod foreground;
mod hamlib;
pub mod keyer_interface;
pub mod mfj1278;
mod netkeyer;
pub mod newtlf;
mod qtcutil;
pub mod weakstubs;
pub mod workqueue;
pub mod write_keyer;
