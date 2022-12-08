use std::{
    ffi::{c_char, CStr},
    str::FromStr,
};

mod tlf {
    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]
    #![allow(unused)]
    #![allow(improper_ctypes)]
    #![allow(clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub mod background_process;
mod cw_utils;
pub mod err_utils;
mod foreground;
mod netkeyer;
mod qtcutil;
pub mod write_keyer;

pub(crate) unsafe fn parse_cstr<T: FromStr>(s: *const c_char) -> Option<T> {
    CStr::from_ptr(s)
        .to_str()
        .ok()
        .and_then(|t| str::parse::<T>(t).ok())
}
