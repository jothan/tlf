use std::ffi::{c_char, c_uint, CStr};

const CALLSIGN_SIZE: usize = 14;

/// cbindgen:ptrs-as-arrays=[[callsign; 15]]
#[no_mangle]
pub extern "C" fn parse_qtcline(logline: *const c_char, callsign: *mut c_char, direction: c_uint) {
    let logline = unsafe { CStr::from_ptr(logline) }.to_bytes();

    let callsign =
        unsafe { std::slice::from_raw_parts_mut(callsign as *mut u8, CALLSIGN_SIZE + 1) };

    let offset = match direction {
        crate::tlf::RECV => 30,
        crate::tlf::SEND => 35,
        _ => unreachable!(),
    };
    let source = &logline[offset..offset + CALLSIGN_SIZE];
    let source_end = source
        .iter()
        .position(|c| *c == b' ')
        .unwrap_or(CALLSIGN_SIZE);
    let source_callsign = &source[..source_end];
    callsign[..source_callsign.len()].copy_from_slice(source_callsign);
    callsign[source_callsign.len()] = 0;
}
