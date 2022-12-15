use std::{
    ffi::{c_int, c_uint, c_ulong, CStr, CString},
    fmt::Display,
    mem::MaybeUninit,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant}, borrow::Cow,
};

use libc::{c_char, c_long};
use ptr::Unique;

use crate::{
    background_process::{with_background, BackgroundContext},
    bands::freq2band,
    cw_utils::{GetCWSpeed, SetCWSpeed},
    err_utils::{log_message, showmsg, shownr, LogLevel},
    workqueue::WorkSender,
};

const ENIMPL: c_int = -tlf::RIG_ENIMPL;
const ENAVAIL: c_int = -tlf::RIG_ENAVAIL;

#[derive(Debug)]
pub(crate) struct RigConfig {
    model: tlf::rig_model_t,
    portname: Option<CString>,
    serial_rate: c_int,
    rigconf: Vec<(CString, CString)>,
    use_keyer: bool,
    cw_bandwidth: Option<tlf::pbwidth_t>,
    want_ptt: bool,
    trxmode: c_uint,
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

static USE_PTT: AtomicBool = AtomicBool::new(false);

impl From<c_int> for GenericError {
    fn from(code: c_int) -> GenericError {
        GenericError(code)
    }
}

impl Display for GenericError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        with_rigerror(self.0, |msg| write!(f, "{msg}"))
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

fn retval_to_result(retval: c_int) -> Result<(), GenericError> {
    if retval == tlf::RIG_OK {
        Ok(())
    } else {
        Err(retval.into())
    }
}

fn result_to_retval(result: Result<(), GenericError>) -> c_int {
    match result {
        Ok(_) => tlf::RIG_OK,
        Err(e) => e.0,
    }
}

pub(crate) struct Rig {
    handle: Unique<tlf::RIG>,
    opened: bool,
    can_send_morse: bool,
    can_stop_morse: bool,
    cw_bandwidth: Option<tlf::pbwidth_t>,
    use_ptt: bool,
    ptt_state: bool,
    use_keyer: bool,
    state: Option<RigState>,
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
            want_ptt: tlf::rigptt,
            trxmode: tlf::trxmode as c_uint,
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

        for directive in rigconf.split(',') {
            let (param, value) = directive.split_once('=').ok_or(Error::InvalidRigconf)?;
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
        let has_ptt = caps.ptt_type == tlf::ptt_type_t_RIG_PTT_RIG;

        if self.want_ptt && !has_ptt {
            showmsg!("Controlling PTT via Hamlib is not supported for that rig!");
        }
        let use_ptt = has_ptt && self.want_ptt;
        USE_PTT.fetch_or(use_ptt, Ordering::SeqCst);

        let can_send_morse = caps.send_morse.is_some();
        let can_stop_morse = caps.stop_morse.is_some();

        let mut rig = Rig {
            handle: rig,
            can_send_morse,
            can_stop_morse,
            opened: false,
            cw_bandwidth: self.cw_bandwidth,
            use_ptt,
            ptt_state: false,
            use_keyer: self.use_keyer,
            state: None,
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

        // Initialize RIG_VFO_CURR
        let rigfreq = match rig.get_vfo() {
            Ok(_) | Err(GenericError(ENIMPL)) | Err(GenericError(ENAVAIL)) => rig.get_freq(),
            Err(e) => Err(e),
        }
        .map_err(print_error)?;

        shownr!("Freq =", rigfreq as c_int);

        if self.use_keyer {
            // Set the initial speed from the current radio setting
            let rig_speed = rig.get_keyer_speed()?;
            SetCWSpeed(rig_speed);
            let rounded_speed = GetCWSpeed();
            if rig_speed != rounded_speed {
                rig.set_keyer_speed(rounded_speed)?;
            }
        }

        match self.trxmode {
            tlf::SSBMODE => set_outfreq(tlf::SETSSBMODE as _),
            tlf::DIGIMODE => set_outfreq(tlf::SETDIGIMODE as _),
            tlf::CWMODE => set_outfreq(tlf::SETCWMODE as _),
            _ => (),
        }
        Ok(rig)
    }
}

impl Rig {
    fn get_keyer_speed(&mut self) -> Result<c_uint, GenericError> {
        let mut value = MaybeUninit::uninit();

        let retval = unsafe {
            tlf::rig_get_level(
                self.handle.as_mut(),
                tlf::RIG_VFO_CURR,
                tlf::RIG_LEVEL_KEYSPD,
                value.as_mut_ptr(),
            )
        };
        retval_to_result(retval)
            .map(|_| unsafe { value.assume_init().i as c_uint })
            .map_err(|e| {
                log_message(
                    LogLevel::WARN,
                    format!("Could not read CW speed from rig : {e}"),
                );
                e
            })
    }

    fn set_keyer_speed(&mut self, speed: c_uint) -> Result<(), GenericError> {
        let value = tlf::value_t { i: speed as c_int };

        let retval = unsafe {
            tlf::rig_set_level(
                self.handle.as_mut(),
                tlf::RIG_VFO_CURR,
                tlf::RIG_LEVEL_KEYSPD,
                value,
            )
        };
        retval_to_result(retval)
    }

    pub(crate) fn keyer_send(&mut self, message: impl AsRef<CStr>) -> Result<(), GenericError> {
        let retval = unsafe {
            tlf::rig_send_morse(
                self.handle.as_mut(),
                tlf::RIG_VFO_CURR,
                message.as_ref().as_ptr(),
            )
        };
        retval_to_result(retval)
    }

    fn stop_keyer(&mut self) -> Result<(), GenericError> {
        if !self.can_stop_morse {
            return Ok(());
        }

        let retval = unsafe { tlf::rig_stop_morse(self.handle.as_mut(), tlf::RIG_VFO_CURR) };
        retval_to_result(retval)
    }

    fn get_mode(&mut self) -> Result<(tlf::rmode_t, tlf::pbwidth_t), GenericError> {
        let mut mode: tlf::rmode_t = tlf::RIG_MODE_NONE.into();
        let mut bandwidth = 0;

        let retval = unsafe {
            tlf::rig_get_mode(
                self.handle.as_mut(),
                tlf::RIG_VFO_CURR,
                &mut mode,
                &mut bandwidth,
            )
        };
        retval_to_result(retval).map(|_| (mode, bandwidth))
    }

    fn set_mode(
        &mut self,
        mode: tlf::rmode_t,
        bandwidth: Option<tlf::pbwidth_t>,
    ) -> Result<(), GenericError> {
        let retval = unsafe {
            tlf::rig_set_mode(
                self.handle.as_mut(),
                tlf::RIG_VFO_CURR,
                mode,
                bandwidth.unwrap_or(tlf::RIG_PASSBAND_NOCHANGE),
            )
        };
        retval_to_result(retval)
    }

    fn set_cw_mode(&mut self) -> Result<(), GenericError> {
        self.set_mode(tlf::RIG_MODE_CW, self.cw_bandwidth)
    }

    fn set_ssb_mode(&mut self, freq: tlf::freq_t) -> Result<(), GenericError> {
        self.set_mode(get_ssb_mode(freq), None)
    }

    fn reset_rit(&mut self) -> Result<(), GenericError> {
        let retval = unsafe { tlf::rig_set_rit(self.handle.as_mut(), tlf::RIG_VFO_CURR, 0) };
        retval_to_result(retval)
    }

    fn set_ptt(&mut self, ptt: bool) -> Result<(), GenericError> {
        if !self.use_ptt || self.ptt_state == ptt {
            return Ok(());
        }

        let hl_ptt = if ptt {
            tlf::ptt_t_RIG_PTT_ON
        } else {
            tlf::ptt_t_RIG_PTT_OFF
        };

        let retval = unsafe { tlf::rig_set_ptt(self.handle.as_mut(), tlf::RIG_VFO_CURR, hl_ptt) };
        retval_to_result(retval)?;

        self.ptt_state = ptt;
        Ok(())
    }

    fn get_vfo(&mut self) -> Result<tlf::vfo_t, GenericError> {
        let mut vfo = 0;
        let retval = unsafe { tlf::rig_get_vfo(self.handle.as_mut(), &mut vfo) };
        retval_to_result(retval).map(|_| vfo)
    }

    fn get_freq(&mut self) -> Result<tlf::freq_t, GenericError> {
        let mut freq = 0.;
        let retval =
            unsafe { tlf::rig_get_freq(self.handle.as_mut(), tlf::RIG_VFO_CURR, &mut freq) };
        retval_to_result(retval).map(|_| freq)
    }

    fn set_freq(&mut self, freq: tlf::freq_t) -> Result<(), GenericError> {
        let retval = unsafe { tlf::rig_set_freq(self.handle.as_mut(), tlf::RIG_VFO_CURR, freq) };
        retval_to_result(retval)
    }

    pub(crate) fn poll(&mut self) {
        let previous = self.state.take();
        self.state = Some(RigState::poll(self, previous));
    }

    pub(crate) fn can_send_morse(&self) -> bool {
        self.can_send_morse
    }

    pub(crate) fn can_stop_morse(&self) -> bool {
        self.can_stop_morse
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
        let vfo_result = rig.get_vfo().map(|vfo| {
            out.vfo = Some(vfo);
            vfo
        });
        match vfo_result {
            Ok(_) | Err(GenericError(ENIMPL)) | Err(GenericError(ENAVAIL)) => {
                if let Ok(freq) = rig.get_freq() {
                    out.freq = Some(freq);
                }
            }
            _ => (),
        };

        if out.freq.is_some() {
            if let Ok((mode, bandwidth)) = rig.get_mode() {
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

        if out.change_freq(rig, previous.as_ref()).is_err() {
            return out;
        }

        if rig.use_keyer {
            match rig.get_keyer_speed() {
                Ok(rig_speed) => {
                    if GetCWSpeed() != rig_speed {
                        // Should the rounded wpm value be written back to the radio if different ?
                        SetCWSpeed(rig_speed);
                        let new_speed = GetCWSpeed();

                        if new_speed != rig_speed {
                            // TODO: send this to main thread
                            unsafe {
                                tlf::display_cw_speed(GetCWSpeed());
                            }
                        }
                    }
                }
                Err(e) => {
                    print_error(e);
                }
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

        let freq = self.freq.unwrap();

        if freq >= unsafe { tlf::bandcorner[0][0] } as tlf::freq_t {
            unsafe { tlf::freq = freq };
        }

        self.bandidx = freq2band(freq as c_uint);

        // Handle this by subscribing to the above state update
        unsafe { tlf::bandfrequency[self.bandidx.unwrap_or(tlf::BANDINDEX_OOB as usize)] = freq };

        let oldbandidx = previous.and_then(|s| s.bandidx);

        if self.bandidx != oldbandidx {
            // band change on trx
            unsafe { handle_trx_bandswitch(rig, self.mode, freq) }.map_err(print_error)?;
        }

        Ok(())
    }
}

/// Safety: full of global state references here
unsafe fn handle_trx_bandswitch(
    rig: &mut Rig,
    rigmode: Option<tlf::rmode_t>,
    freq: tlf::freq_t,
) -> Result<(), GenericError> {
    unsafe { tlf::send_bandswitch(freq) };

    let mut mode: Option<tlf::rmode_t> = None; // default: no change
    let mut width = None; // passband width, in Hz

    if tlf::trxmode == tlf::SSBMODE as c_int {
        mode = Some(get_ssb_mode(freq));
    } else if tlf::trxmode == tlf::DIGIMODE as c_int {
        let rigmode = rigmode.unwrap_or(tlf::RIG_MODE_NONE as tlf::rmode_t);
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

    if let Some(mode) = mode {
        rig.set_mode(mode, width)?;
    }

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

fn with_rigerror<F: FnOnce(Cow<str>) -> T, T>(error: c_int, f: F) -> T {
    // rigerror uses an internal static, non threadsafe, buffer
    static RIGERROR_LOCK: Mutex<()> = Mutex::new(());

    let _ugly = RIGERROR_LOCK.lock();
    let msg = unsafe { CStr::from_ptr(tlf::rigerror(error)) }.to_string_lossy();
    f(msg)
}

#[no_mangle]
pub extern "C" fn set_outfreq(hertz: tlf::freq_t) {
    if !unsafe { tlf::trx_control } {
        return; // no rig control, ignore request
    }

    if hertz > 0. {
        let mut hertz = hertz - unsafe { tlf::fldigi_get_carrier() as tlf::freq_t };
        if hertz < 0. {
            hertz = 0.;
        }

        with_background(|bg| {
            bg.schedule_nowait(move |rig| {
                let _ = rig.as_mut().unwrap().set_freq(hertz).map_err(print_error);
            })
            .expect("background send error")
        });
    } else if hertz < 0. {
        with_background(|bg| outfreq_request(hertz, bg));
    }
}

#[no_mangle]
pub extern "C" fn set_outfreq_wait(hertz: tlf::freq_t) {
    let hertz = hertz - unsafe { tlf::fldigi_get_carrier() as tlf::freq_t };
    assert!(hertz >= 0.);

    with_background(|bg| {
        bg.schedule_wait(move |rig| {
            let _ = rig.as_mut().unwrap().set_freq(hertz).map_err(print_error);
        })
        .expect("background send error")
    });
}

fn outfreq_request(hertz: tlf::freq_t, bg: &WorkSender<BackgroundContext>) {
    let request: i32 = hertz as _;

    match request {
        tlf::SETCWMODE => bg.schedule_nowait(|rig| {
            let _ = rig.as_mut().unwrap().set_cw_mode().map_err(print_error);
        }),

        tlf::SETSSBMODE => bg.schedule_nowait(move |rig| {
            let _ = rig
                .as_mut()
                .unwrap()
                .set_ssb_mode(hertz)
                .map_err(print_error);
        }),

        tlf::SETDIGIMODE => {
            let mut mode: tlf::rmode_t = unsafe { tlf::digi_mode };
            let is_fldigi = unsafe { tlf::digikeyer } == tlf::FLDIGI as c_int;

            if mode == tlf::RIG_MODE_NONE as c_ulong {
                if is_fldigi {
                    mode = tlf::RIG_MODE_USB;
                } else {
                    mode = tlf::RIG_MODE_LSB;
                }
            }

            bg.schedule_nowait(move |rig| {
                let _ = rig
                    .as_mut()
                    .unwrap()
                    .set_mode(mode, None)
                    .map_err(print_error);
            })
        }

        tlf::RESETRIT => bg.schedule_nowait(|rig| {
            let _ = rig.as_mut().unwrap().reset_rit().map_err(print_error);
        }),

        _ => panic!("Unknown set_outfreq request: {request}"),
    }
    .expect("background send error");
}

#[no_mangle]
pub extern "C" fn hamlib_keyer_set_speed(cwspeed: c_int) -> c_int {
    let set_result = with_background(|bg| {
        bg.schedule_wait(move |rig| rig.as_mut().unwrap().set_keyer_speed(cwspeed as c_uint))
            .expect("background send error")
    });
    result_to_retval(set_result)
}

#[no_mangle]
pub unsafe extern "C" fn hamlib_keyer_stop() -> c_int {
    let stop_result = with_background(|bg| {
        bg.schedule_wait(|rig| rig.as_mut().unwrap().stop_keyer())
            .expect("background send error")
    });
    result_to_retval(stop_result)
}

#[no_mangle]
pub unsafe extern "C" fn hamlib_use_ptt() -> bool {
    USE_PTT.load(Ordering::SeqCst)
}

#[no_mangle]
pub unsafe extern "C" fn hamlib_set_ptt(ptt: bool) -> c_int {
    let ptt_result = with_background(|bg| {
        bg.schedule_wait(move |rig| rig.as_mut().unwrap().set_ptt(ptt))
            .expect("background send error")
    });
    result_to_retval(ptt_result)
}

fn print_error(e: GenericError) -> GenericError {
    log_message(LogLevel::WARN, format!("Problem with rig link: {e}"));
    e
}