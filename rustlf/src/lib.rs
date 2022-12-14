// unsafe, unsafe everywhere
#![allow(clippy::missing_safety_doc)]

use std::{
    ffi::{c_char, CStr},
    str::FromStr,
};

pub mod background_process;
pub mod bands;
mod cw_utils;
pub mod err_utils;
mod foreground;
mod hamlib;
mod netkeyer;
mod qtcutil;
pub mod workqueue;
pub mod write_keyer;

pub(crate) unsafe fn parse_cstr<T: FromStr>(s: *const c_char) -> Option<T> {
    CStr::from_ptr(s)
        .to_str()
        .ok()
        .and_then(|t| str::parse::<T>(t).ok())
}
