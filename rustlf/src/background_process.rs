use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::sync::{Arc, Condvar, Mutex};

use std::time::Duration;

use crate::err_utils::{log_message, LogLevel};
use crate::foreground::{ForegroundContext, BACKGROUND_HANDLE, FOREGROUND_HANDLE};
use crate::hamlib::Rig;
use crate::netkeyer::{Netkeyer, NETKEYER};
use crate::workqueue::{WorkSender, Worker};
use crate::write_keyer::{write_keyer, KeyerConsumer};

struct StopFlags {
    stopped: bool,
    stop_request: bool,
}

static STOP_PROCESS: Mutex<StopFlags> = Mutex::new(StopFlags {
    stopped: false,
    stop_request: true,
});
static START_COND: Condvar = Condvar::new();
static STOPPED_COND: Condvar = Condvar::new();

#[no_mangle]
pub extern "C" fn stop_background_process() {
    let mut s = STOP_PROCESS.lock().unwrap();
    s.stop_request = true;

    let _s = STOPPED_COND.wait_while(s, |s| !s.stopped).unwrap();
}

#[no_mangle]
pub extern "C" fn start_background_process() {
    let mut s = STOP_PROCESS.lock().unwrap();
    s.stop_request = false;
    START_COND.notify_all();
}

#[no_mangle]
pub extern "C" fn is_background_process_stopped() -> bool {
    STOP_PROCESS.lock().unwrap().stop_request
}

fn background_process_wait() {
    let mut s = STOP_PROCESS.lock().unwrap();

    if s.stop_request {
        s.stopped = true;
        STOPPED_COND.notify_all();
        s = START_COND.wait_while(s, |s| s.stop_request).unwrap();
        s.stopped = false;
    }
}

pub(crate) struct BackgroundConfig {
    pub(crate) keyer_consumer: KeyerConsumer,
    pub(crate) netkeyer: Arc<Option<Netkeyer>>,
    pub(crate) worker: Worker<BackgroundContext>,
    pub(crate) fg_producer: WorkSender<ForegroundContext>,
    pub(crate) rig: Option<Rig>,
}

#[no_mangle]
pub unsafe extern "C" fn background_process(config: *mut c_void) -> *mut c_void {
    let BackgroundConfig {
        mut keyer_consumer,
        netkeyer,
        worker,
        mut rig,
        fg_producer,
    } = *Box::from_raw(config as *mut BackgroundConfig);
    FOREGROUND_HANDLE.with(|fg| *fg.borrow_mut() = Some(fg_producer));

    let netkeyer = (*netkeyer).as_ref();

    let mut lantimesync: c_int = 0;
    let mut fldigi_rpc_cnt: bool = false;

    loop {
        background_process_wait();
        if worker
            .process_sleep(&mut rig, Duration::from_millis(10))
            .is_err()
        {
            // Exit thread when disconnected.
            break std::ptr::null_mut();
        }

        unsafe { tlf::receive_packet() };
        unsafe { tlf::rx_rtty() };

        /*
         * calling Fldigi XMLRPC method, which reads the Fldigi's carrier:
         * fldigi_xmlrpc_get_carrier()
         * this function helps to show the correct freq of the RIG: reads
         * the carrier value from Fldigi, and stores in a variable; then
         * it readable by fldigi_get_carrier()
         * only need at every 2nd cycle
         * see fldigixmlrpc.[ch]
         *
         * There are two addition routines
         *   fldigi_get_log_call() reads the callsign, if user clicks to a string in Fldigi's RX window
         *   fldigi_get_log_serial_number() reads the exchange
         */
        if tlf::digikeyer == tlf::FLDIGI as _ && tlf::fldigi_isenabled() && rig.is_some() {
            if fldigi_rpc_cnt {
                tlf::fldigi_xmlrpc_get_carrier();
                tlf::fldigi_get_log_call();
                tlf::fldigi_get_log_serial_number();
            }
            fldigi_rpc_cnt = !fldigi_rpc_cnt;
        }

        if !is_background_process_stopped() {
            tlf::cqww_simulator();
            write_keyer(&mut keyer_consumer, rig.as_mut(), netkeyer);
        }

        tlf::handle_lan_recv(&mut lantimesync);

        // get freq info from TRX
        if let Some(rig) = rig.as_mut() {
            let _ = rig.poll().map_err(|e| {
                log_message(LogLevel::WARN, format!("Problem reading radio status: {e}"));
            });
        }
    }
}

pub(crate) type BackgroundContext = Option<Rig>;

pub(crate) struct PlaySoundConfig {
    pub(crate) netkeyer: Arc<Option<Netkeyer>>,
    pub(crate) bg_thread: Option<WorkSender<BackgroundContext>>,
    pub(crate) audiofile: CString,
}

#[no_mangle]
pub unsafe extern "C" fn prepare_playsound(audiofile: *const c_char) -> *mut c_void {
    let netkeyer = NETKEYER.with(|fg_netkeyer| fg_netkeyer.borrow().clone());
    let bg_thread = BACKGROUND_HANDLE.with(|bg_thread| bg_thread.borrow().clone());

    let audiofile = CStr::from_ptr(audiofile).to_owned();
    fn assert_send<T: Send>() {}
    let _ = assert_send::<PlaySoundConfig>;
    let config = Box::new(PlaySoundConfig {
        netkeyer,
        bg_thread,
        audiofile,
    });
    Box::into_raw(config) as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn abort_playsound(config: *mut c_void) {
    std::mem::drop(Box::from_raw(config as *mut PlaySoundConfig));
}

#[no_mangle]
pub unsafe extern "C" fn init_playsound(config: *mut c_void) -> *mut c_char {
    let PlaySoundConfig {
        netkeyer,
        bg_thread,
        audiofile,
    } = *Box::from_raw(config as *mut PlaySoundConfig);
    NETKEYER.with(|audio_netkeyer| *audio_netkeyer.borrow_mut() = netkeyer);
    BACKGROUND_HANDLE.with(|audio_bg| *audio_bg.borrow_mut() = bg_thread);

    audiofile.into_raw()
}

#[no_mangle]
pub unsafe extern "C" fn close_playsound(audiofile: *mut c_char) {
    std::mem::drop(CString::from_raw(audiofile));
}

pub(crate) fn with_background<F: FnOnce(&WorkSender<BackgroundContext>) -> T, T>(f: F) -> T {
    BACKGROUND_HANDLE.with(|bg| {
        let bg = bg.borrow();
        let bg = bg.as_ref().expect("called from wrong thread");
        f(bg)
    })
}

pub(crate) fn with_foreground<F: FnOnce(&WorkSender<ForegroundContext>) -> T, T>(f: F) -> T {
    FOREGROUND_HANDLE.with(|fg| {
        let fg = fg.borrow();
        let fg = fg.as_ref().expect("called from wrong thread");
        f(fg)
    })
}
