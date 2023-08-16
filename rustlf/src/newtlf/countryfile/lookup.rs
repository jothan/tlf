use std::borrow::Cow;

/* replace callsign area (K2ND/4 -> K4ND)
 *
 * for stations with multiple digits (LZ1000) it replaces the last digit
 * (may be wrong)
 */
fn change_area(call: &mut str, area: u8) {
    // Safety: only modifying an ascii value
    let call = unsafe { call.as_bytes_mut() };
    assert!(area.is_ascii_digit());

    if let Some(orig_area) = call.iter_mut().rev().find(|c| c.is_ascii_digit()) {
        *orig_area = area;
    }
}

pub(crate) fn strip_call(mut call: &str) -> &str {
    if call.ends_with("/QRP") {
        call = &call[0..call.len() - 4];
    }

    // check for calls which have no assigned country and no assigned zone, e.g. airborne mobile /AM or maritime mobile /MM
    if call.ends_with("/MM") || call.ends_with("/AM") {
        call = "";
    }
    call
}

pub(crate) fn normalize_call(call: &str) -> (Cow<str>, bool) {
    let mut abnormal = false;

    let mut call = Cow::from(call);
    let mut checkbuffer = String::new();

    if let Some((call1, call2)) = call.split_once('/') {
        let mut loc = call1.len();
        if call2.len() < call1.len() && call2.len() > 1 {
            let mut c = String::from(call2);
            c.push('/');
            c.push_str(call1);
            abnormal = true;

            call = c.into();
            loc = call.find('/').unwrap();
        }

        if loc > 3 {
            let (left, right) = call.split_at(loc);
            checkbuffer = right[1..].to_string();
            if checkbuffer.len() == 1 {
                call = left.to_owned().into();
            }
        }

        if let Some(loc) = call.find('/') {
            if loc < 5 {
                call.to_mut().truncate(loc);
            }
        }

        if checkbuffer.len() == 1 && checkbuffer.as_bytes()[0].is_ascii_digit() {
            change_area(call.to_mut(), checkbuffer.as_bytes()[0]);
        } else if checkbuffer.len() > 1 {
            call = checkbuffer.into();
        }
    }

    (call, abnormal)
}
