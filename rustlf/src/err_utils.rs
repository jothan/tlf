use std::ffi::CStr;
use std::thread::sleep;
use std::time::Duration;

pub enum LogLevel {
    DEBUG,
    INFO,
    WARN,
    ERR,
}

pub fn log_message(level: LogLevel, message: impl AsRef<CStr>) {
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

#[repr(i32)]
pub enum CResult {
    Ok = 0,
    Err = -1,
}

impl <T, E> From<Result<T, E>> for CResult {
    fn from(result: Result<T, E>) -> CResult {
        match result {
            Ok(_) => CResult::Ok,
            Err(_) => CResult::Err,
        }
    }
}

impl <T> From<Option<T>> for CResult {
    fn from(option: Option<T>) -> CResult {
        match option {
            Some(_) => CResult::Ok,
            None => CResult::Err,
        }
    }
}