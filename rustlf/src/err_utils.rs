use std::ffi::{c_int, CStr, CString};
use std::thread::sleep;
use std::time::Duration;

pub enum LogLevel {
    DEBUG,
    INFO,
    WARN,
    ERR,
}

pub fn log_message_raw(level: LogLevel, message: impl AsRef<CStr>) {
    unsafe {
        let lines = tlf::LINES;
        tlf::clear_line(lines - 1);
        tlf::mvaddstr(lines - 1, 0, message.as_ref().as_ptr());
        tlf::refreshp();

        match level {
            LogLevel::DEBUG => (),
            LogLevel::INFO => sleep(Duration::from_secs(1)),
            LogLevel::WARN => sleep(Duration::from_secs(3)),

            LogLevel::ERR => {
                sleep(Duration::from_secs(3));
                tlf::exit(tlf::EXIT_FAILURE as i32);
            }
        }
    }
}

pub fn log_message_string(level: LogLevel, message: impl Into<String>) {
    let message = message.into();
    log_message_raw(level, CString::new(message).expect("invalid message"));
}

macro_rules! log_message {
    ($level:expr,$msg:literal) => {
        $crate::err_utils::log_message_raw($level, cstr::cstr!($msg));
    };
    ($level:expr,$msg:expr) => {
        $crate::err_utils::log_message_string($level, $msg);
    };
}

pub(crate) use log_message;

macro_rules! showmsg {
    ($msg:literal) => {
        unsafe { tlf::showmsg(cstr::cstr!($msg).as_ptr()) }
    };
    ($msg:expr) => {
        let s = std::ffi::CString::new($msg).expect("invalid message");
        unsafe { tlf::showmsg(s.as_ptr()) }
    };
}

pub(crate) use showmsg;

macro_rules! shownr {
    ($msg:literal, $nr:expr) => {
        unsafe { tlf::shownr(cstr::cstr!($msg).as_ptr(), $nr) }
    };
}

pub(crate) use shownr;

use crate::background_process::exec_foreground;

#[repr(i32)]
pub enum CResult {
    Ok = 0,
    Err = -1,
}

impl<T, E> From<Result<T, E>> for CResult {
    fn from(result: Result<T, E>) -> CResult {
        match result {
            Ok(_) => CResult::Ok,
            Err(_) => CResult::Err,
        }
    }
}

impl<T> From<Option<T>> for CResult {
    fn from(option: Option<T>) -> CResult {
        match option {
            Some(_) => CResult::Ok,
            None => CResult::Err,
        }
    }
}

pub(crate) fn switch_to_ssb() {
    exec_foreground(|| {
        log_message!(LogLevel::WARN, "keyer not active; switching to SSB");
        unsafe {
            tlf::trxmode = tlf::SSBMODE as c_int;
            tlf::clear_display();
        }
    });
}
