use std::{
    ffi::{c_int, c_uint},
    ops::DerefMut,
};

use crate::{
    cw_utils::{decrease_cw_speed, increase_cw_speed, GetCWSpeed},
    err_utils::{self, log_message, switch_to_ssb, CResult},
    foreground::KEYER_INTERFACE,
    newtlf::netkeyer::Error,
};

pub trait CwKeyerFrontend {
    fn set_speed(&mut self, _speed: c_uint) -> Result<(), Error> {
        Ok(())
    }

    fn set_weight(&mut self, _weight: c_int) -> Result<(), Error> {
        Ok(())
    }

    fn set_tone(&mut self, _tone: u16) -> Result<(), Error> {
        Ok(())
    }

    fn stop_keying(&mut self) -> Result<(), Error> {
        Ok(())
    }

    fn reset(&mut self) -> Result<(), Error> {
        Ok(())
    }

    fn name(&self) -> &'static str;
}

pub struct NullKeyer;

impl CwKeyerFrontend for NullKeyer {
    fn name(&self) -> &'static str {
        "null"
    }
}

pub trait CwKeyerBackend {
    fn prepare_message(&self, _msg: &mut Vec<u8>) {}

    fn send_message(&mut self, _msg: Vec<u8>) -> Result<(), Error> {
        Ok(())
    }
}

pub(crate) fn with_keyer_interface<R, F: FnOnce(&mut dyn CwKeyerFrontend) -> R>(f: F) -> R {
    KEYER_INTERFACE.with_borrow_mut(|keyer| {
        let keyer = keyer.as_mut().expect("called keyer from the wrong thread");
        f(keyer.deref_mut())
    })
}

fn set_cw_speed() {
    with_keyer_interface(|keyer| match keyer.set_speed(GetCWSpeed()) {
        Ok(_) => {}
        Err(_) => {
            log_message!(err_utils::LogLevel::WARN, "Could not set CW speed");
            unsafe { tlf::clear_display() };
        }
    });
}

/// Increase the CW speed in 2 wpm increments
#[no_mangle]
pub extern "C" fn speedup() {
    if unsafe { tlf::trxmode != tlf::CWMODE as c_int } {
        return;
    }

    increase_cw_speed();
    set_cw_speed();
}

/// Decrease the CW speed in 2 wpm increments
#[no_mangle]
pub extern "C" fn speeddown() {
    if unsafe { tlf::trxmode != tlf::CWMODE as c_int } {
        return;
    }

    decrease_cw_speed();
    set_cw_speed();
}

#[no_mangle]
pub extern "C" fn setweight(weight: c_int) -> CResult {
    with_keyer_interface(|keyer| match keyer.set_weight(weight) {
        Ok(_) => CResult::Ok,
        Err(_) => {
            log_message!(err_utils::LogLevel::INFO, "Keyer not active?");
            unsafe { tlf::clear_display() };
            CResult::Err
        }
    })
}

#[no_mangle]
pub extern "C" fn stoptx_cw() {
    with_keyer_interface(|keyer| match keyer.stop_keying() {
        Ok(_) => {}
        Err(_) => {
            switch_to_ssb();
            unsafe { tlf::clear_display() };
        }
    });
}

#[no_mangle]
pub extern "C" fn cwkeyer_reset() {
    with_keyer_interface(|keyer| match keyer.reset() {
        Ok(_) => {}
        Err(_) => {
            switch_to_ssb();
            unsafe { tlf::clear_display() };
        }
    });
}
