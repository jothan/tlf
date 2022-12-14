use std::ffi::{CStr, CString};
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

pub fn log_message(level: LogLevel, message: impl Into<String>) {
    let message = message.into();
    log_message_raw(level, CString::new(message).expect("invalid message"));
}

macro_rules! log_message_static {
    ($level:expr,$msg:literal) => {
        $crate::err_utils::log_message_raw(
            $level,
            CStr::from_bytes_with_nul(concat!($msg, "\x00").as_bytes()).expect("invalid message"),
        );
    };
}

pub(crate) use log_message_static;

macro_rules! showmsg {
    ($msg:literal) => {
        unsafe { tlf::showmsg(concat!($msg, "\0").as_ptr() as *const c_char) }
    };
}

pub(crate) use showmsg;

macro_rules! shownr {
    ($msg:literal, $nr:expr) => {
        unsafe { tlf::shownr(concat!($msg, "\0").as_ptr() as *const c_char, $nr) }
    };
}

pub(crate) use shownr;

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
