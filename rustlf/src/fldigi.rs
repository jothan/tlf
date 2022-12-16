use std::cell::Cell;
use std::ffi::c_int;

use crate::hamlib::{GenericError, Rig};

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

pub(crate) fn apply_shift_freq(
    rig: &mut Rig,
    mode: Option<tlf::rmode_t>,
    mut freq: tlf::freq_t,
) -> Result<(), GenericError> {
    if mode != Some(tlf::RIG_MODE_RTTY) && mode != Some(tlf::RIG_MODE_RTTYR) {
        return Ok(());
    }
    let shift = VAR_SHIFT_FREQ.with(|f| f.take());
    if shift == 0 {
        return Ok(());
    }
    let shift = VAR_SHIFT_FREQ.with(|f| f.take());
    freq += tlf::freq_t::from(shift);

    rig.set_freq(freq)
}
