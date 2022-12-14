use std::{
    ffi::{c_int, c_uint, CStr, CString},
    fmt::Display,
    mem::MaybeUninit,
    sync::Mutex,
    time::{Duration, Instant},
};

use libc::{c_char, c_long};
use ptr::Unique;

use crate::{
    bands::freq2band,
    cw_utils::{GetCWSpeed, SetCWSpeed},
    err_utils::{log_message, showmsg, shownr, LogLevel},
};

#[derive(Debug)]
pub(crate) struct RigConfig {
    model: tlf::rig_model_t,
    portname: Option<CString>,
    serial_rate: c_int,
    rigconf: Vec<(CString, CString)>,
    use_keyer: bool,
    cw_bandwidth: Option<tlf::pbwidth_t>,
    want_ptt: bool,
}

#[derive(Debug)]
struct RigState {
    vfo: Option<tlf::vfo_t>,
    freq: Option<tlf::freq_t>,
    bandwidth: Option<tlf::pbwidth_t>,
    mode: Option<tlf::rmode_t>,
    bandidx: Option<usize>,
    time: Instant,
}

#[derive(Debug)]
pub(crate) enum Error {
    Generic(GenericError),
    InvalidRigconf,
    InvalidModel,
    Open(c_int),
}

#[derive(Debug)]
pub(crate) struct GenericError(c_int);

impl From<c_int> for GenericError {
    fn from(code: c_int) -> GenericError {
        GenericError(code)
    }
}

impl Display for GenericError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        with_rigerror(self.0, |msg| write!(f, "{}", msg))
    }
}

impl From<GenericError> for Error {
    fn from(error: GenericError) -> Error {
        Error::Generic(error)
    }
}

impl From<c_int> for Error {
    fn from(code: c_int) -> Error {
        Error::Generic(GenericError(code))
    }
}

pub(crate) struct Rig {
    handle: Unique<tlf::RIG>,
    opened: bool,
    can_send_morse: bool,
    can_stop_morse: bool,
    cw_bandwidth: Option<tlf::pbwidth_t>,
    has_ptt: bool,
    use_ptt: bool,
    ptt_state: bool,
    use_keyer: bool,
}

unsafe impl Send for Rig {}

impl Drop for Rig {
    fn drop(&mut self) {
        unsafe {
            // TODO: log or handle errors
            if self.opened {
                tlf::rig_close(self.handle.as_mut());
                self.opened = false;
            }
            tlf::rig_cleanup(self.handle.as_mut());
            self.handle = Unique::empty();
        }
    }
}

impl RigConfig {
    pub(crate) unsafe fn from_globals() -> Result<RigConfig, Error> {
        let model = tlf::myrig_model as tlf::rig_model_t;

        let portname = if tlf::rigportname.is_null() {
            None
        } else {
            let s = CStr::from_ptr(tlf::rigportname);
            if s.to_bytes().is_empty() {
                None
            } else {
                let mut s = s.to_owned().into_bytes();
                // Remove final newline
                if s.last() == Some(&b'\n') {
                    s.pop();
                }
                Some(CString::new(s).unwrap())
            }
        };
        // TODO: add a way to configure dcd and ptt, it is dead code in the original.

        let cw_bandwidth = Some(tlf::cw_bandwidth as c_long).filter(|b| *b > 0);

        Ok(RigConfig {
            model,
            portname,
            serial_rate: tlf::serial_rate,
            rigconf: RigConfig::parse_rigconf()?,
            use_keyer: tlf::cwkeyer == tlf::HAMLIB_KEYER as c_int,
            cw_bandwidth,
            want_ptt: tlf::rigptt as c_uint & tlf::CAT_PTT_WANTED != 0,
        })
    }

    unsafe fn parse_rigconf() -> Result<Vec<(CString, CString)>, Error> {
        let rigconf = CStr::from_ptr(&tlf::rigconf as *const c_char)
            .to_str()
            .map_err(|_| Error::InvalidRigconf)?;
        let mut out = Vec::new();

        if rigconf.is_empty() {
            return Ok(out);
        }

        for directive in rigconf.split(",") {
            let (param, value) = directive.split_once("=").ok_or(Error::InvalidRigconf)?;
            if param.is_empty() {
                return Err(Error::InvalidRigconf);
            }
            // Impossible to have an interior nul at this point.
            let param = CString::new(param.to_owned()).unwrap();
            let value = CString::new(value.to_owned()).unwrap();
            out.push((param, value));
        }

        Ok(out)
    }

    pub(crate) fn open_rig(&self) -> Result<Rig, Error> {
        let rig: *mut tlf::RIG = unsafe { tlf::rig_init(self.model) };
        let mut rig = match Unique::new(rig) {
            Some(rig) => rig,
            None => return Err(Error::InvalidModel),
        };

        if let Some(ref portname) = self.portname {
            assert!(portname.to_bytes_with_nul().len() < tlf::HAMLIB_FILPATHLEN as usize);
            unsafe {
                let rig = rig.as_mut();
                libc::strncpy(
                    &mut rig.state.rigport.pathname as *mut c_char,
                    portname.as_ptr(),
                    tlf::HAMLIB_FILPATHLEN as usize,
                );
            }
        }

        let caps = unsafe { &*rig.as_ref().caps };
        /* If CAT PTT is wanted, test for CAT capability of rig backend. */
        let has_ptt;

        unsafe {
            has_ptt = caps.ptt_type == tlf::ptt_type_t_RIG_PTT_RIG;

            if self.want_ptt && !has_ptt {
                showmsg!("Controlling PTT via Hamlib is not supported for that rig!");
            }
        }

        let can_send_morse = caps.send_morse.is_some();
        let can_stop_morse = caps.stop_morse.is_some();

        let mut rig = Rig {
            handle: rig,
            can_send_morse,
            can_stop_morse,
            opened: false,
            cw_bandwidth: self.cw_bandwidth,
            has_ptt,
            use_ptt: has_ptt && self.want_ptt,
            ptt_state: false,
            use_keyer: self.use_keyer,
        };

        let rig_mut = unsafe { rig.handle.as_mut() };

        rig_mut.state.rigport.parm.serial.rate = self.serial_rate;

        for (param, value) in &self.rigconf {
            unsafe {
                let token = tlf::rig_token_lookup(rig_mut, param.as_ptr());
                if token as c_uint == tlf::RIG_CONF_END {
                    return Err(Error::InvalidRigconf);
                }

                let retval = tlf::rig_set_conf(rig_mut, token, value.as_ptr());
                if retval != tlf::rig_errcode_e_RIG_OK as c_int {
                    return Err(retval.into());
                }
            }
        }

        let retval = unsafe { tlf::rig_open(rig.handle.as_mut()) };
        if retval != tlf::rig_errcode_e_RIG_OK as c_int {
            return Err(Error::Open(retval));
        }
        rig.opened = true;

        let mut rigfreq: tlf::freq_t = 0.0;
        let mut vfo: tlf::vfo_t = 0;

        let mut retval = unsafe { tlf::rig_get_vfo(rig.handle.as_mut(), &mut vfo) }; /* initialize RIG_VFO_CURR */
        if retval == tlf::RIG_OK || retval == -tlf::RIG_ENIMPL || retval == -tlf::RIG_ENAVAIL {
            retval =
                unsafe { tlf::rig_get_freq(rig.handle.as_mut(), tlf::RIG_VFO_CURR, &mut rigfreq) };
        }

        if retval != tlf::RIG_OK {
            return Err(retval.into());
        }

        shownr!("Freq =", rigfreq as c_int);

        if self.use_keyer {
            // Set the initial speed from the current radio setting
            crate::cw_utils::SetCWSpeed(rig.get_keyer_speed()?);
        }

        // TODO: do proper mode setting
        rig.set_cw_mode()?;

        Ok(rig)
    }
}

impl Rig {
    pub(crate) fn get_keyer_speed(&mut self) -> Result<c_uint, GenericError> {
        let mut value = MaybeUninit::uninit();

        let retval = unsafe {
            tlf::rig_get_level(
                self.handle.as_mut(),
                tlf::RIG_VFO_CURR,
                tlf::RIG_LEVEL_KEYSPD,
                value.as_mut_ptr(),
            )
        };

        if retval == tlf::RIG_OK {
            Ok(unsafe { value.assume_init().i as c_uint })
        } else {
            let e = retval.into();
            log_message(
                LogLevel::WARN,
                format!("Could not read CW speed from rig : {}", e),
            );
            Err(e)
        }
    }

    pub(crate) fn set_keyer_speed(&mut self, speed: c_uint) -> Result<(), GenericError> {
        let value = tlf::value_t { i: speed as c_int };

        let retval = unsafe {
            tlf::rig_set_level(
                self.handle.as_mut(),
                tlf::RIG_VFO_CURR,
                tlf::RIG_LEVEL_KEYSPD,
                value,
            )
        };

        if retval == tlf::RIG_OK {
            Ok(())
        } else {
            Err(retval.into())
        }
    }

    pub(crate) fn keyer_send(&mut self, message: impl AsRef<CStr>) -> Result<(), GenericError> {
        let retval = unsafe {
            tlf::rig_send_morse(
                self.handle.as_mut(),
                tlf::RIG_VFO_CURR,
                message.as_ref().as_ptr(),
            )
        };
        if retval == tlf::RIG_OK {
            Ok(())
        } else {
            Err(retval.into())
        }
    }

    pub(crate) fn set_cw_mode(&mut self) -> Result<(), Error> {
        let retval = unsafe {
            tlf::rig_set_mode(
                self.handle.as_mut(),
                tlf::RIG_VFO_CURR,
                tlf::RIG_MODE_CW,
                self.cw_bandwidth.unwrap_or(tlf::RIG_PASSBAND_NOCHANGE),
            )
        };

        if retval == tlf::RIG_OK {
            Ok(())
        } else {
            Err(retval.into())
        }
    }

    pub(crate) fn set_ptt(&mut self, ptt: bool) -> Result<(), Error> {
        if !self.use_ptt || self.ptt_state == ptt {
            return Ok(());
        }

        let hl_ptt = if ptt {
            tlf::ptt_t_RIG_PTT_ON
        } else {
            tlf::ptt_t_RIG_PTT_OFF
        };

        let retval;
        unsafe {
            retval = tlf::rig_set_ptt(self.handle.as_mut(), tlf::RIG_VFO_CURR, hl_ptt);
        }

        if retval == tlf::RIG_OK {
            self.ptt_state = ptt;
            Ok(())
        } else {
            Err(retval.into())
        }
    }
}

impl RigState {
    const POLL_PERIOD: Duration = Duration::from_millis(200);

    fn poll(rig: &mut Rig, previous: Option<RigState>) -> RigState {
        let mut out = RigState {
            time: Instant::now(),
            vfo: None,
            freq: None,
            mode: None,
            bandwidth: None,
            bandidx: None,
        };

        if let Some(p) = previous.as_ref() {
            if out.time.duration_since(p.time) < Self::POLL_PERIOD {
                return previous.unwrap();
            }
        }

        // Initialize RIG_VFO_CURR
        let mut vfo = 0;
        let retval = unsafe { tlf::rig_get_vfo(rig.handle.as_mut(), &mut vfo) };
        if retval == tlf::RIG_OK {
            out.vfo = Some(vfo);
        }

        if out.vfo.is_some() || retval == -tlf::RIG_ENIMPL || retval == -tlf::RIG_ENAVAIL {
            let mut freq = 0.;
            let retval =
                unsafe { tlf::rig_get_freq(rig.handle.as_mut(), tlf::RIG_VFO_CURR, &mut freq) };
            if retval == tlf::RIG_OK {
                out.freq = Some(freq);
            }
        }

        if out.freq.is_some() {
            let mut mode = 0;
            let mut bandwidth = 0;
            let retval = unsafe {
                tlf::rig_get_mode(
                    rig.handle.as_mut(),
                    tlf::RIG_VFO_CURR,
                    &mut mode,
                    &mut bandwidth,
                )
            };
            if retval == tlf::RIG_OK {
                out.mode = Some(mode);
                out.bandwidth = Some(bandwidth);
            }
        }

        /* TODO: fldigi handling
            if (trxmode == DIGIMODE && (digikeyer == GMFSK || digikeyer == FLDIGI)) {
            rigfreq += (freq_t)fldigi_get_carrier();
            if (rigmode == RIG_MODE_RTTY || rigmode == RIG_MODE_RTTYR) {
            fldigi_shift_freq = fldigi_get_shift_freq();
            if (fldigi_shift_freq != 0) {
                pthread_mutex_lock(&rig_lock);
                retval = rig_set_freq(my_rig, RIG_VFO_CURR,
                          ((freq_t)rigfreq + (freq_t)fldigi_shift_freq));
                pthread_mutex_unlock(&rig_lock);
            }
            }
        } */

        if let Err(_) = out.change_freq(rig, previous.as_ref()) {
            return out;
        }

        if rig.use_keyer {
            match rig.get_keyer_speed() {
                Ok(rig_speed) => {
                    if GetCWSpeed() != rig_speed {
                        // Should the rounded wpm value be written back to the radio if different ?
                        SetCWSpeed(rig_speed);
                    }

                    // TODO: send this to main thread
                    unsafe {
                        tlf::display_cw_speed(GetCWSpeed());
                    }
                }
                Err(e) => log_message(LogLevel::WARN, format!("Problem with rig link: {}", e)),
            }
        }

        out
    }

    fn change_freq(&mut self, rig: &mut Rig, previous: Option<&RigState>) -> Result<(), Error> {
        // TODO: broadcast frequency properly from here
        if self.freq.is_none() {
            unsafe { tlf::freq = 0. };
            return Err(GenericError(-1).into());
        }

        let freq = self.freq.clone().unwrap();

        if freq >= unsafe { tlf::bandcorner[0][0] } as tlf::freq_t {
            unsafe { tlf::freq = freq };
        }

        self.bandidx = freq2band(freq as c_uint);

        // Handle this by subscribing to the above state update
        unsafe { tlf::bandfrequency[self.bandidx.unwrap_or(tlf::BANDINDEX_OOB as usize)] = freq };

        let oldbandidx = previous.map(|s| s.bandidx).flatten();

        if self.bandidx != oldbandidx {
            // band change on trx
            if let Err(e) = unsafe { handle_trx_bandswitch(rig, self, freq) } {
                log_message(LogLevel::WARN, format!("Problem with rig link: {}", e));
            }
        }

        Ok(())
    }
}

/// Safety: full of global state references here
unsafe fn handle_trx_bandswitch(
    rig: &mut Rig,
    state: &mut RigState,
    freq: tlf::freq_t,
) -> Result<(), GenericError> {
    unsafe { tlf::send_bandswitch(freq) };

    let mut mode: Option<tlf::rmode_t> = None; // default: no change
    let mut width = None; // passband width, in Hz

    if tlf::trxmode == tlf::SSBMODE as c_int {
        mode = Some(get_ssb_mode(freq));
    } else if tlf::trxmode == tlf::DIGIMODE as c_int {
        let rigmode = state.mode.unwrap_or(tlf::RIG_MODE_NONE as tlf::rmode_t);
        if rigmode
            & (tlf::RIG_MODE_LSB | tlf::RIG_MODE_USB | tlf::RIG_MODE_RTTY | tlf::RIG_MODE_RTTYR)
            != rigmode
        {
            mode = Some(tlf::RIG_MODE_LSB);
        }
    } else {
        mode = Some(tlf::RIG_MODE_CW);
        width = rig.cw_bandwidth;
    }

    if mode.is_none() {
        return Ok(()); // no change was requested
    }

    let retval = tlf::rig_set_mode(
        rig.handle.as_mut(),
        tlf::RIG_VFO_CURR,
        mode.unwrap(),
        width.unwrap_or(tlf::RIG_PASSBAND_NOCHANGE),
    );

    if retval != tlf::RIG_OK {
        return Err(retval.into());
    }
    state.mode = mode;
    state.bandwidth = width.or(state.bandwidth);

    Ok(())
}

fn get_ssb_mode(freq: tlf::freq_t) -> tlf::rmode_t {
    let freq = freq as c_uint;
    // LSB below 14 MHz, USB above it
    if freq < unsafe { tlf::bandcorner[tlf::BANDINDEX_20 as usize][0] } {
        tlf::RIG_MODE_LSB
    } else {
        tlf::RIG_MODE_USB
    }
}

static RIGERROR_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn rigerror(error: c_int) -> String {
    // rigerror uses a non threadsafe buffer
    let _ugly = RIGERROR_LOCK.lock();
    let msg = unsafe { CStr::from_ptr(tlf::rigerror(error)) };
    msg.to_string_lossy().into_owned()
}

pub(crate) fn with_rigerror<F: FnOnce(&str) -> T, T>(error: c_int, f: F) -> T {
    // rigerror uses a non threadsafe buffer
    let _ugly = RIGERROR_LOCK.lock();
    let msg = unsafe { CStr::from_ptr(tlf::rigerror(error)) }.to_string_lossy();
    f(msg.as_ref())
}
