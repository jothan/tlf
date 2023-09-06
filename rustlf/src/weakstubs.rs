#![allow(non_upper_case_globals)]
//! Stubs for the test harnesses when C TLF is not fully linked together.

use std::ffi::{c_char, c_uint};

#[linkage = "weak"]
#[no_mangle]
pub extern "C" fn sendmessage(_msg: *const c_char) {}

#[linkage = "weak"]
#[no_mangle]
pub static bandcorner: [[c_uint; 2]; tlf::NBANDS as usize] = [
    [1800000, 2000000], // band bottom, band top
    [3500000, 4000000],
    [5250000, 5450000], // 5351500-5356500 worldwide
    [7000000, 7300000],
    [10100000, 10150000],
    [14000000, 14350000],
    [18068000, 18168000],
    [21000000, 21450000],
    [24890000, 24990000],
    [28000000, 29700000],
    [0, 0],
];
