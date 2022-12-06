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
        let lines = crate::tlf::LINES;
        crate::tlf::clear_line(lines - 1);
        crate::tlf::mvaddstr(lines - 1, 0, message.as_ref().as_ptr());
        crate::tlf::refreshp();

        match level {
            LogLevel::DEBUG => (),
            LogLevel::INFO => sleep(Duration::from_secs(1)),
            LogLevel::WARN => sleep(Duration::from_secs(3)),

            LogLevel::ERR => {
                sleep(Duration::from_secs(3));
                crate::tlf::exit(crate::tlf::EXIT_FAILURE as i32);
            }
        }
    }
}
