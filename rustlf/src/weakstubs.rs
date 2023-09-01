//! Stubs for the test harnesses when C TLF is not fully linked together.

use std::ffi::c_char;

#[linkage = "weak"]
#[no_mangle]
pub extern "C" fn sendmessage(_msg: *const c_char) {}
