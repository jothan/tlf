use std::{
    ffi::{c_uint, CStr, CString, OsStr},
    io::Write,
    os::unix::prelude::OsStrExt,
};

use crate::{
    err_utils::switch_to_ssb,
    foreground::fg_usleep,
    keyer_interface::{CwKeyerBackend, CwKeyerFrontend},
};
use cstr::cstr;
use newtlf::netkeyer::Error;

pub struct Mfj1278Keyer;

impl CwKeyerFrontend for Mfj1278Keyer {
    fn name(&self) -> &'static str {
        "MFJ 1278"
    }

    fn set_speed(&mut self, speed: c_uint) -> Result<(), Error> {
        unsafe { tlf::sendmessage(cstr!("\\\x0d").as_ptr()) };
        fg_usleep(500000);

        let msg = CString::new(format!("MSP {speed: >2} \x0d")).unwrap();
        unsafe { tlf::sendmessage(msg.as_ptr()) };
        fg_usleep(500000);

        unsafe { tlf::sendmessage(cstr!("CONV\x0d\n").as_ptr()) };

        Ok(())
    }
}

impl CwKeyerBackend for Mfj1278Keyer {
    fn send_message(&mut self, msg: Vec<u8>) -> Result<(), Error> {
        let path = unsafe { CStr::from_ptr(&tlf::controllerport as *const i8) };
        let path = OsStr::from_bytes(path.to_bytes());
        let file_open = std::fs::File::options()
            .append(true)
            .create(false)
            .open(path);
        match file_open {
            Ok(mut file) => {
                // FIXME: should this be silent ?
                let _ = file.write_all(&msg);
            }
            Err(_) => switch_to_ssb(),
        };

        Ok(())
    }

    fn prepare_message(&self, msg: &mut Vec<u8>) {
        for b in msg.iter_mut() {
            if *b == b'\n' {
                *b = b'\r';
            }
        }
    }
}
