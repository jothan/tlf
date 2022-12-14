#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(unused)]
#![allow(improper_ctypes)]
#![allow(clippy::all)]

use std::ffi::{c_int, c_long, c_uint, c_ulong};
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

pub const RIG_OK: c_int = rig_errcode_e_RIG_OK as c_int;
pub const RIG_ENIMPL: c_int = rig_errcode_e_RIG_ENIMPL as c_int;
pub const RIG_ENAVAIL: c_int = rig_errcode_e_RIG_ENAVAIL as c_int;

pub const RIG_VFO_CURR: c_uint = 1 << 29;
pub const RIG_LEVEL_KEYSPD: c_ulong = 1 << 14;
pub const RIG_PASSBAND_NORMAL: shortfreq_t = 0;
pub const RIG_PASSBAND_NOCHANGE: shortfreq_t = -1;
pub const RIG_MODE_CW: rmode_t = 1 << 1;
pub const RIG_MODE_USB: rmode_t = 1 << 2;
pub const RIG_MODE_LSB: rmode_t = 1 << 3;
pub const RIG_MODE_SSB: rmode_t = RIG_MODE_USB | RIG_MODE_LSB;
pub const RIG_MODE_RTTY: rmode_t = 1 << 4;
pub const RIG_MODE_RTTYR: rmode_t = 1 << 8;
