use std::ffi::{c_char, c_uint, CStr};

const CALLSIGN_SIZE: usize = 14;
const QTC_RECV_OFFSET: usize = 30;
const QTC_SEND_OFFSET: usize = 35;

/// cbindgen:ptrs-as-arrays=[[callsign; 15]]
#[no_mangle]
pub unsafe extern "C" fn parse_qtcline(
    logline: *const c_char,
    callsign: *mut c_char,
    direction: c_uint,
) {
    let logline = CStr::from_ptr(logline).to_bytes();
    let callsign = std::slice::from_raw_parts_mut(callsign as *mut u8, CALLSIGN_SIZE + 1);

    parse_qtcline_inner(logline, callsign, direction);
}

#[inline]
fn parse_qtcline_inner(logline: &[u8], callsign: &mut [u8], direction: c_uint) {
    let offset = match direction {
        tlf::RECV => QTC_RECV_OFFSET,
        tlf::SEND => QTC_SEND_OFFSET,
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

#[cfg(test)]
mod tests {
    use std::ffi::CStr;

    use super::{parse_qtcline_inner, CALLSIGN_SIZE, QTC_RECV_OFFSET};
    const QTC_LENGTH: usize = 120;

    fn make_line(callsign_length: usize, logline: &mut Vec<u8>) {
        logline.clear();
        logline.extend(std::iter::repeat(b' ').take(QTC_RECV_OFFSET));
        logline.extend(std::iter::repeat(b'A').take(callsign_length));
        logline.extend(std::iter::repeat(b' ').take(QTC_LENGTH - logline.len()));
        logline[QTC_LENGTH - 1] = b'\n';
    }

    #[test]
    fn test_parse_qtcline() {
        const TEST_SIZE: usize = 20;
        let mut logline = Vec::with_capacity(QTC_LENGTH);
        let mut callsign = [255; CALLSIGN_SIZE + 1];

        for i in 0..TEST_SIZE {
            make_line(i, &mut logline);
            callsign.fill(255);

            parse_qtcline_inner(
                logline.as_slice(),
                callsign.as_mut_slice(),
                tlf::RECV,
            );

            let callsign = unsafe { CStr::from_ptr(callsign.as_ptr() as *const i8) };
            assert_eq!(callsign.to_bytes().len(), std::cmp::min(i, CALLSIGN_SIZE));
        }
    }
}
