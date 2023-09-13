use std::ffi::c_uint;

pub(crate) fn freq2bandindex(freq: c_uint) -> Option<usize> {
    let corners = unsafe { &tlf::bandcorner };

    for (i, [bottom, top]) in corners.iter().enumerate() {
        if (bottom..=top).contains(&&freq) {
            return Some(i); // in band
        }
    }

    // Not in any band
    None
}
