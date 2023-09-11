use std::cell::RefCell;
use std::ffi::{c_char, c_int, c_uint, CStr};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;

use crate::cw_utils::GetCWSpeed;
use crate::err_utils::{log_message, CResult};
use crate::foreground::exec_foreground;
use crate::keyer_interface::{with_keyer_interface, CwKeyerBackend, CwKeyerFrontend};
use newtlf::netkeyer::{Error, Netkeyer};

thread_local! {
    pub(crate) static NETKEYER: RefCell<Option<Arc<Netkeyer>>> = RefCell::new(None);
}

const DEFAULT_TONE: u16 = 600;
// Could be owned by the main thread if the simulator did not set it.
static TONE: AtomicU16 = AtomicU16::new(DEFAULT_TONE);
/// Grab keyer parameters from the global variables

pub(crate) unsafe fn netkeyer_from_globals() -> Result<Netkeyer, Error> {
    let host = unsafe { CStr::from_ptr(&tlf::netkeyer_hostaddress as *const c_char) };
    let port = unsafe { tlf::netkeyer_port as c_uint }.try_into().unwrap();
    let netkeyer =
        Netkeyer::from_host_and_port(host.to_str().map_err(|_| Error::InvalidDevice)?, port)?;

    netkeyer.reset()?;
    netkeyer.set_weight(tlf::weight as i8)?;

    netkeyer.write_tone(get_tone())?;

    netkeyer.set_speed(GetCWSpeed().try_into().unwrap())?;
    netkeyer.set_weight(tlf::weight as _)?;

    let keyer_device = CStr::from_ptr(&tlf::keyer_device as *const c_char);

    if !keyer_device.to_bytes().is_empty() {
        netkeyer.set_device(keyer_device.to_bytes())?;
    }

    netkeyer.set_tx_delay(tlf::txdelay as _)?;
    if tlf::sc_sidetone {
        netkeyer.set_sidetone_device(b's')?;
    }

    let sc_volume = Some(tlf::sc_volume).and_then(|v| v.try_into().ok());
    if let Some(sc_volume) = sc_volume {
        netkeyer.set_sidetone_volume(sc_volume)?;
    }

    Ok(netkeyer)
}

#[no_mangle]
pub unsafe extern "C" fn parse_tone(tonestr: *const c_char) -> u16 {
    CStr::from_ptr(tonestr)
        .to_str()
        .ok()
        .map(str::trim)
        .and_then(|t| t.parse::<u16>().ok())
        .unwrap_or(DEFAULT_TONE)
}

#[no_mangle]
pub extern "C" fn init_tone(tone: u16) {
    TONE.store(tone, Ordering::Release);
}

#[no_mangle]
pub extern "C" fn get_tone() -> u16 {
    TONE.load(Ordering::Acquire)
}

#[no_mangle]
pub extern "C" fn write_tone(tone: u16) -> u16 {
    let prev_tone = TONE.swap(tone, Ordering::AcqRel);

    exec_foreground(move || {
        with_keyer_interface(|keyer| {
            if let Err(e) = keyer.set_tone(tone) {
                log_message!(
                    crate::err_utils::LogLevel::INFO,
                    format!("Could not set tone: {e:?}")
                )
            }
        });
    });

    prev_tone
}

#[no_mangle]
pub extern "C" fn netkeyer_set_ptt(ptt: bool) -> CResult {
    with_netkeyer(|netkeyer| netkeyer.set_ptt(ptt))
}

#[no_mangle]
pub extern "C" fn netkeyer_abort() -> CResult {
    with_netkeyer(|netkeyer| netkeyer.abort())
}

#[no_mangle]
pub extern "C" fn netkeyer_set_pin14(pin14: bool) -> CResult {
    with_netkeyer(|netkeyer| netkeyer.set_pin14(pin14))
}

#[no_mangle]
pub extern "C" fn netkeyer_tune(seconds: c_uint) -> CResult {
    with_netkeyer(|netkeyer| {
        seconds
            .try_into()
            .ok()
            .and_then(|speed| netkeyer.tune(speed).ok())
    })
}

#[no_mangle]
pub extern "C" fn netkeyer_set_band_switch(bandidx: c_uint) -> CResult {
    with_netkeyer(|netkeyer| {
        bandidx
            .try_into()
            .ok()
            .and_then(|bandidx| netkeyer.set_band_switch(bandidx).ok())
    })
}

#[no_mangle]
pub extern "C" fn netkeyer_enable_word_mode() -> CResult {
    with_netkeyer(|netkeyer| netkeyer.enable_word_mode())
}

#[no_mangle]
pub extern "C" fn netkeyer_set_sidetone_volume(volume: c_uint) -> CResult {
    with_netkeyer(|netkeyer| {
        volume
            .try_into()
            .ok()
            .and_then(|volume| netkeyer.set_sidetone_volume(volume).ok())
    })
}

fn with_netkeyer<R: Into<CResult>, F: FnOnce(&Netkeyer) -> R>(f: F) -> CResult {
    NETKEYER.with_borrow(|netkeyer| {
        if let Some(netkeyer) = netkeyer {
            f(netkeyer).into()
        } else {
            CResult::Err
        }
    })
}

pub struct NetKeyerFrontend(Arc<Netkeyer>);

impl NetKeyerFrontend {
    pub(crate) fn new(netkeyer: Arc<Netkeyer>) -> NetKeyerFrontend {
        NetKeyerFrontend(netkeyer)
    }
}

impl CwKeyerFrontend for NetKeyerFrontend {
    fn name(&self) -> &'static str {
        "cwdaemon"
    }

    fn set_speed(&mut self, speed: c_uint) -> Result<(), Error> {
        let speed = speed.try_into().map_err(|_| Error::InvalidParameter)?;
        self.0.set_speed(speed)
    }

    fn set_weight(&mut self, weight: c_int) -> Result<(), Error> {
        let weight = weight.try_into().map_err(|_| Error::InvalidParameter)?;
        self.0.set_weight(weight)
    }

    fn set_tone(&mut self, tone: u16) -> Result<(), Error> {
        self.0.write_tone(tone)
    }

    fn stop_keying(&mut self) -> Result<(), Error> {
        self.0.abort()
    }

    fn reset(&mut self) -> Result<(), Error> {
        self.0.reset()
    }
}

impl CwKeyerBackend for Arc<Netkeyer> {
    fn send_message(&mut self, msg: Vec<u8>) -> Result<(), Error> {
        self.send_text(&msg)
    }
}
