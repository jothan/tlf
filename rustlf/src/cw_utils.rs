use std::ffi::{c_char, c_uint, CStr};
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

static SPEED: AtomicUsize = AtomicUsize::new(10);

static SPEEDS: [c_uint; 21] = [
    6, 12, 14, 16, 18, 20, 22, 24, 26, 28, 30, 32, 34, 36, 38, 40, 42, 44, 46, 48, 50,
];

/* Get CW speed
 *
 * Return the actual CW speed in WPM as integer
 * \return The CW speed in WPM
 */
#[no_mangle]
pub extern "C" fn GetCWSpeed() -> c_uint {
    SPEEDS[SPEED.load(Ordering::SeqCst)].try_into().unwrap()
}

#[no_mangle]
pub extern "C" fn GetCWSpeedIndex() -> c_uint {
    SPEED.load(Ordering::SeqCst).try_into().unwrap()
}

#[no_mangle]
pub extern "C" fn SetCWSpeed(wpm: c_uint) {
    SPEED.store(speed_conversion(wpm), Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn DecreaseCWSpeed() {
    SPEED
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |mut speed| {
            if speed > 0 {
                speed -= 1;
            }
            Some(speed)
        })
        .unwrap();
}

#[no_mangle]
pub extern "C" fn IncreaseCWSpeed() {
    SPEED
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |mut speed| {
            if speed < SPEEDS.len() - 1 {
                speed += 1;
            }
            Some(speed)
        })
        .unwrap();
}

/* converts cw speed in wpm to an numbered index into speedstr table */
fn speed_conversion(cwspeed: c_uint) -> usize {
    SPEEDS
        .iter()
        .position(|speed| cwspeed <= *speed)
        .unwrap_or(SPEEDS.len() - 1)
}

/** calculate dot length of a cw message
 *
 * Calculate the length of a given CW message in dot elements.
 * Expands '%' into your own call.
 * \param message the CW message
 * \return number of dot elements in the message
 */
#[no_mangle]
pub extern "C" fn cw_message_length(message: *const c_char, mycall: *const c_char) -> c_uint {
    let message = unsafe { CStr::from_ptr(message) };
    let mycall = unsafe { CStr::from_ptr(mycall) };

    message
        .to_bytes()
        .iter()
        .copied()
        .map(|c| {
            if c == b'%' {
                mycall
                    .to_bytes()
                    .iter()
                    .copied()
                    .map(|c| getCWdots(c.try_into().unwrap()))
                    .sum()
            } else {
                getCWdots(c.try_into().unwrap())
            }
        })
        .sum()
}

/** get length of CW characters
 *
 * converts a given CW character into the number of dot elements
 * \param ch the character to convert
 * \return number of dots for the character including the following character
 *         space
 */
#[no_mangle]
pub extern "C" fn getCWdots(ch: c_char) -> c_uint {
    match ch.try_into().unwrap() {
        b'A' => 9,
        b'B' => 13,
        b'C' => 15,
        b'D' => 11,
        b'E' => 5,
        b'F' => 13,
        b'G' => 13,
        b'H' => 11,
        b'I' => 7,
        b'J' => 17,
        b'K' => 13,
        b'L' => 13,
        b'M' => 11,
        b'N' => 9,
        b'O' => 15,
        b'P' => 15,
        b'Q' => 17,
        b'R' => 11,
        b'S' => 9,
        b'T' => 7,
        b'U' => 11,
        b'V' => 13,
        b'W' => 13,
        b'X' => 15,
        b'Y' => 17,
        b'Z' => 15,
        b'0' => 23,
        b'1' => 21,
        b'2' => 19,
        b'3' => 17,
        b'4' => 15,
        b'5' => 13,
        b'6' => 15,
        b'7' => 17,
        b'8' => 19,
        b'9' => 21,
        b'/' => 17,
        b'?' => 19,
        b' ' => 3,
        _ => 0,
    }
}
