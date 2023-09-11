use std::ffi::{c_int, c_void};
use std::sync::{Arc, Condvar, Mutex};

use std::thread::JoinHandle;
use std::time::Duration;

use cstr::cstr;

use crate::cqww_simulator::CqwwSimulator;
use crate::err_utils::{log_message, LogLevel};
use crate::foreground::{ForegroundContext, BACKGROUND_HANDLE, FOREGROUND_HANDLE};
use crate::hamlib::Rig;
use crate::workqueue::{WorkSender, Worker};
use crate::write_keyer::{write_keyer, KeyerConsumer};
use newtlf::netkeyer::Netkeyer;

struct StopFlags {
    stopped: bool,
    stop_request: bool,
    exit_request: bool,
}

static STOP_PROCESS: Mutex<StopFlags> = Mutex::new(StopFlags {
    stopped: false,
    stop_request: true,
    exit_request: false,
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

fn exit_background_process() {
    let mut s = STOP_PROCESS.lock().unwrap();
    s.stop_request = false;
    s.exit_request = true;
    START_COND.notify_all();
}

#[no_mangle]
pub extern "C" fn is_background_process_stopped() -> bool {
    STOP_PROCESS.lock().unwrap().stop_request
}

fn background_process_wait() -> bool {
    let mut s = STOP_PROCESS.lock().unwrap();

    if s.stop_request {
        s.stopped = true;
        STOPPED_COND.notify_all();
        s = START_COND.wait_while(s, |s| s.stop_request).unwrap();
        s.stopped = false;
    }
    s.exit_request
}

pub(crate) struct BackgroundConfig {
    pub(crate) keyer_consumer: KeyerConsumer,
    pub(crate) netkeyer: Option<Arc<Netkeyer>>,
    pub(crate) worker: Worker<BackgroundContext>,
    pub(crate) fg_producer: WorkSender<ForegroundContext>,
    pub(crate) rig: Option<Rig>,
}

unsafe fn background_process(config: BackgroundConfig) {
    let BackgroundConfig {
        mut keyer_consumer,
        mut netkeyer,
        worker,
        rig,
        fg_producer,
    } = config;
    FOREGROUND_HANDLE.with_borrow_mut(|fg| *fg = Some(fg_producer));

    let mut context = BackgroundContext {
        rig,
        simulator: CqwwSimulator::new(),
    };

    let mut lantimesync: c_int = 0;
    let mut fldigi_rpc_cnt: bool = false;

    loop {
        if background_process_wait() {
            break;
        }

        if worker
            .process_sleep(&mut context, Duration::from_millis(10))
            .is_err()
        {
            // Exit thread when disconnected.
            break;
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
        if tlf::digikeyer == tlf::FLDIGI as _ && tlf::fldigi_isenabled() && context.rig.is_some() {
            if fldigi_rpc_cnt {
                tlf::fldigi_xmlrpc_get_carrier();
                tlf::fldigi_get_log_call();
                tlf::fldigi_get_log_serial_number();
            }
            fldigi_rpc_cnt = !fldigi_rpc_cnt;
        }

        if !is_background_process_stopped() {
            write_keyer(&mut keyer_consumer, context.rig.as_mut(), netkeyer.as_mut());
        }

        tlf::handle_lan_recv(&mut lantimesync);

        // get freq info from TRX
        if let Some(rig) = context.rig.as_mut() {
            let _ = rig.poll().map_err(|e| {
                log_message!(LogLevel::WARN, format!("Problem reading radio status: {e}"));
            });
        }
    }
}

pub(crate) struct BackgroundContext {
    pub(crate) rig: Option<Rig>,
    pub(crate) simulator: CqwwSimulator,
}

pub(crate) fn with_background<F: FnOnce(&WorkSender<BackgroundContext>) -> T, T>(f: F) -> T {
    BACKGROUND_HANDLE.with_borrow(|bg| {
        let bg = bg.as_ref().expect("called from wrong thread");
        f(bg)
    })
}

type BackgroundThread = (tlf::pthread_t, JoinHandle<()>);

#[no_mangle]
pub unsafe extern "C" fn spawn_background_thread(config: *mut c_void) -> *mut c_void {
    let config: BackgroundConfig = *Box::from_raw(config as *mut BackgroundConfig);
    let fg_id = tlf::pthread_self();

    match std::thread::Builder::new()
        .name("background".to_owned())
        .spawn(|| background_process(config))
    {
        Ok(j) => {
            let out: Box<BackgroundThread> = Box::new((fg_id, j));
            Box::into_raw(out) as *mut c_void
        }
        Err(_) => {
            tlf::perror(cstr!("pthread_create: backgound_process").as_ptr());
            tlf::endwin();
            std::process::exit(1);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn join_background_thread(join_handle: *mut c_void) {
    exit_background_process();

    if join_handle.is_null() {
        return;
    }
    let (fg_id, join_handle): BackgroundThread =
        *Box::from_raw(join_handle as *mut BackgroundThread);

    if tlf::pthread_equal(tlf::pthread_self(), fg_id) != 0 {
        join_handle.join().expect("background thread panicked");
    }
}
