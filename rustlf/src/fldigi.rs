use std::cell::Cell;
use std::ffi::c_int;

thread_local! {
    pub(crate) static VAR_SHIFT_FREQ: Cell<c_int>  = Cell::new(0);
}

#[no_mangle]
pub unsafe extern "C" fn fldigi_var_shift_freq_get() -> c_int {
    VAR_SHIFT_FREQ.with(|f| f.get())
}

#[no_mangle]
pub unsafe extern "C" fn fldigi_var_shift_freq_set(freq: c_int) {
    VAR_SHIFT_FREQ.with(|f| f.set(freq))
}

pub(crate) fn get_shifted_freq(
    mode: Option<tlf::rmode_t>,
) -> Option<tlf::freq_t> {
    if mode != Some(tlf::RIG_MODE_RTTY) && mode != Some(tlf::RIG_MODE_RTTYR) {
        return None;
    }
    let shift = VAR_SHIFT_FREQ.with(|f| f.take());
    if shift == 0 {
        return None;
    }
    Some(shift.into())
}
