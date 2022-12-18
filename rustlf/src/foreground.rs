use std::cell::RefCell;
use std::ffi::{c_int, c_uint, c_ulong, c_void};
use std::sync::Arc;
use std::time::Duration;

use crate::background_process::BackgroundContext;
use crate::err_utils::{showmsg, shownr};
use crate::hamlib::{set_outfreq, Error, Rig, RigConfig};
use crate::netkeyer::Netkeyer;
use crate::workqueue::{workqueue, WorkSender, Worker};
use crate::{background_process::BackgroundConfig, write_keyer::keyer_queue_init};

const BACKGROUND_QUEUE_SIZE: usize = 16;
const FOREGROUND_QUEUE_SIZE: usize = 16;

pub(crate) type ForegroundContext = ();

thread_local! {
    pub(crate) static BACKGROUND_HANDLE: RefCell<Option<WorkSender<BackgroundContext>>> = RefCell::new(None);
    pub(crate) static FOREGROUND_HANDLE: RefCell<Option<WorkSender<ForegroundContext>>> = RefCell::new(None);
    pub(crate) static FOREGROUND_WORKER: RefCell<Option<Worker<ForegroundContext>>> = RefCell::new(None);
}

#[no_mangle]
pub extern "C" fn foreground_init() -> *mut c_void {
    let (bg_producer, bg_worker) = workqueue::<BackgroundContext>(BACKGROUND_QUEUE_SIZE);
    let (fg_producer, fg_worker) = workqueue::<ForegroundContext>(FOREGROUND_QUEUE_SIZE);
    BACKGROUND_HANDLE.with(|bg| *bg.borrow_mut() = Some(bg_producer));
    FOREGROUND_WORKER.with(|bg| *bg.borrow_mut() = Some(fg_worker));

    let rig = unsafe { hamlib_init().ok() };

    let keyer_consumer = keyer_queue_init();

    let netkeyer = unsafe { keyer_init(&rig) };

    fn assert_send<T: Send>() {}
    let _ = assert_send::<BackgroundConfig>;
    let bg_config = Box::new(BackgroundConfig {
        keyer_consumer,
        netkeyer,
        worker: bg_worker,
        fg_producer,
        rig,
    });
    Box::into_raw(bg_config) as *mut c_void
}

unsafe fn hamlib_init() -> Result<Rig, Error> {
    tlf::rig_set_debug(tlf::rig_debug_level_e_RIG_DEBUG_NONE);

    if !tlf::trx_control {
        return Err(Error::ControlDisabled);
    }

    shownr!("Rig model number is", tlf::myrig_model);
    shownr!("Rig speed is", tlf::serial_rate);

    showmsg!("Trying to start rig control");

    let rig_result = RigConfig::from_globals().and_then(|config| config.open_rig());

    let rig = match rig_result {
        Ok(rig) => rig,
        Err(e) => {
            showmsg!(format!("Could not open rig: {e}"));
            showmsg!("Continue without rig control Y/(N)?");
            if (tlf::key_get() as u8).to_ascii_uppercase() != b'Y' {
                tlf::endwin();
                std::process::exit(1);
            }
            tlf::trx_control = false;
            showmsg!("Disabling rig control!");
            std::thread::sleep(std::time::Duration::from_secs(1));
            return Err(e);
        }
    };

    match tlf::trxmode as c_uint {
        tlf::SSBMODE => set_outfreq(tlf::SETSSBMODE as _),
        tlf::DIGIMODE => set_outfreq(tlf::SETDIGIMODE as _),
        tlf::CWMODE => set_outfreq(tlf::SETCWMODE as _),
        _ => (),
    }

    Ok(rig)
}

unsafe fn keyer_init(rig: &Option<Rig>) -> Arc<Option<Netkeyer>> {
    let netkeyer = if tlf::cwkeyer == tlf::NET_KEYER as _ {
        showmsg!("CW-Keyer is cwdaemon");
        Some(unsafe { Netkeyer::from_globals() }.expect("netkeyer init error"))
    } else {
        None
    };
    let netkeyer = Arc::new(netkeyer);

    crate::netkeyer::NETKEYER.with(|fg_netkeyer| {
        *fg_netkeyer.borrow_mut() = netkeyer.clone();
    });

    if tlf::cwkeyer == tlf::HAMLIB_KEYER as c_int {
        showmsg!("CW-Keyer is Hamlib");
        match rig {
            None => {
                showmsg!("Radio control is not activated!!");
                std::thread::sleep(Duration::from_secs(1));
                tlf::endwin();
                std::process::exit(1);
            }
            Some(rig) => {
                if !rig.can_send_morse() {
                    showmsg!("Rig does not support CW via Hamlib");
                    std::thread::sleep(Duration::from_secs(1));
                    tlf::endwin();
                    std::process::exit(1);
                }
                if !rig.can_stop_morse() {
                    showmsg!("Rig does not support stopping CW!!");
                    showmsg!("Continue anyway Y/(N)?");
                    if (tlf::key_get() as u8).to_ascii_uppercase() != b'Y' {
                        tlf::endwin();
                        std::process::exit(1);
                    }
                }
            }
        }
    }

    if tlf::cwkeyer == tlf::MFJ1278_KEYER as c_int
        || tlf::digikeyer == tlf::MFJ1278_KEYER as c_int
        || tlf::digikeyer == tlf::GMFSK as c_int
    {
        tlf::init_controller();
    }

    netkeyer
}

#[no_mangle]
pub extern "C" fn process_foreground_work() {
    FOREGROUND_WORKER.with(|fg| {
        if let Some(ref fg) = *fg.borrow() {
            fg.process_pending(&mut ())
                .expect("fg worker receive problem");
        }
    })
}

#[inline]
fn fg_sleep_inner(delay: Duration) {
    FOREGROUND_WORKER.with(|fg| {
        if let Some(ref fg) = *fg.borrow() {
            fg.process_sleep(&mut (), delay)
                .expect("fg worker receive problem");
        }
    })
}

#[no_mangle]
pub extern "C" fn fg_usleep(micros: c_ulong) {
    fg_sleep_inner(Duration::from_micros(micros));
}

#[no_mangle]
pub extern "C" fn fg_sleep(secs: c_uint) {
    fg_sleep_inner(Duration::from_secs(secs.into()))
}

pub(crate) fn exec_foreground<F: FnOnce() + Send + 'static>(f: F) {
    if in_foreground() {
        f()
    } else {
        with_foreground(|fg| fg.schedule_nowait(|_| f()).expect("send error"))
    }
}

pub(crate) fn in_foreground() -> bool {
    FOREGROUND_WORKER.with(|fg| fg.borrow().is_some())
}

pub(crate) fn with_foreground<F: FnOnce(&WorkSender<ForegroundContext>) -> T, T>(f: F) -> T {
    FOREGROUND_HANDLE.with(|fg| {
        let fg = fg.borrow();
        let fg = fg.as_ref().expect("called from wrong thread");
        f(fg)
    })
}
