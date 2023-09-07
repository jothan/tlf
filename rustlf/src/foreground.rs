use std::cell::RefCell;
use std::ffi::{c_char, c_int, c_uint, c_ulong, c_void};
use std::sync::Arc;
use std::time::Duration;

use crate::background_process::BackgroundContext;
use crate::err_utils::{showmsg, shownr};
use crate::hamlib::{set_outfreq, Error, HamlibKeyer, Rig, RigConfig};
use crate::keyer_interface::{CwKeyerFrontend, NullKeyer};
use crate::mfj1278::Mfj1278Keyer;
use crate::netkeyer::{NetKeyerFrontend, Netkeyer, NETKEYER};
use crate::workqueue::{workqueue, NoWaitWorkSender, WorkSender, Worker};
use crate::{background_process::BackgroundConfig, write_keyer::keyer_queue_init};

const BACKGROUND_QUEUE_SIZE: Option<usize> = Some(16);
const FOREGROUND_QUEUE_SIZE: Option<usize> = None;

pub(crate) type ForegroundContext = ();

thread_local! {
    pub(crate) static BACKGROUND_HANDLE: RefCell<Option<WorkSender<BackgroundContext>>> = RefCell::new(None);
    pub(crate) static FOREGROUND_HANDLE: RefCell<Option<WorkSender<ForegroundContext>>> = RefCell::new(None);
    pub(crate) static FOREGROUND_WORKER: RefCell<Option<Worker<ForegroundContext>>> = RefCell::new(None);
    pub(crate) static KEYER_INTERFACE: RefCell<Option<Box<dyn CwKeyerFrontend>>> = RefCell::new(None);
}

#[no_mangle]
pub extern "C" fn foreground_init() -> *mut c_void {
    let (bg_producer, bg_worker) = workqueue::<BackgroundContext>(BACKGROUND_QUEUE_SIZE);
    let (fg_producer, fg_worker) = workqueue::<ForegroundContext>(FOREGROUND_QUEUE_SIZE);
    BACKGROUND_HANDLE.with_borrow_mut(|bg| *bg = Some(bg_producer));
    FOREGROUND_WORKER.with_borrow_mut(|bg| *bg = Some(fg_worker));

    let rig = unsafe { hamlib_init().ok() };

    let keyer_consumer = keyer_queue_init();

    let (keyer_interface, netkeyer) = unsafe { keyer_init(&rig) };

    KEYER_INTERFACE.with_borrow_mut(|keyer| *keyer = Some(keyer_interface));
    NETKEYER.with_borrow_mut(|fg_netkeyer| *fg_netkeyer = netkeyer.clone());

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

unsafe fn keyer_init(rig: &Option<Rig>) -> (Box<dyn CwKeyerFrontend>, Option<Arc<Netkeyer>>) {
    let mut netkeyer = None;
    let keyer_interface: Box<dyn CwKeyerFrontend> =
        match (tlf::cwkeyer as c_uint, tlf::digikeyer as c_uint) {
            (tlf::NET_KEYER, _) => {
                showmsg!("CW-Keyer is cwdaemon");
                let netkeyer_raw =
                    Arc::new(unsafe { Netkeyer::from_globals() }.expect("netkeyer init error"));
                netkeyer = Some(netkeyer_raw.clone());

                Box::new(NetKeyerFrontend::new(netkeyer_raw))
            }
            (tlf::HAMLIB_KEYER, _) => {
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
                        if rig.can_stop_morse() {
                            Box::new(HamlibKeyer)
                        } else {
                            showmsg!("Rig does not support stopping CW!!");
                            showmsg!("Continue anyway Y/(N)?");
                            if (tlf::key_get() as u8).to_ascii_uppercase() != b'Y' {
                                tlf::endwin();
                                std::process::exit(1);
                            }
                            Box::new(NullKeyer)
                        }
                    }
                }
            }
            (tlf::MFJ1278_KEYER, _) => {
                tlf::init_controller();
                Box::new(Mfj1278Keyer)
            }
            (_, tlf::MFJ1278_KEYER) | (_, tlf::GMFSK) => {
                tlf::init_controller();
                Box::new(NullKeyer)
            }
            _ => Box::new(NullKeyer),
        };

    (keyer_interface, netkeyer)
}

#[inline]
fn fg_sleep_inner(delay: Duration) {
    FOREGROUND_WORKER.with_borrow(|fg| {
        if let Some(fg) = fg {
            fg.process_sleep(&mut (), delay)
                .expect("fg worker receive problem");
        } else {
            panic!("no fg worker to sleep");
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

#[no_mangle]
pub extern "C" fn getch_process() -> c_int {
    FOREGROUND_WORKER.with_borrow(|fg| {
        let fg = fg.as_ref().unwrap();
        let (c, err) = fg.process_blocking(&mut (), || unsafe { tlf::getch() });

        if let Some(err) = err {
            panic!("Recv error: {:?}", err);
        }
        c
    })
}

struct AssertSend<T>(T);
unsafe impl<T> Send for AssertSend<T> {}

#[no_mangle]
pub extern "C" fn wgetch_process(w: *mut tlf::WINDOW) -> c_int {
    let w = AssertSend(w);

    FOREGROUND_WORKER.with_borrow(|fg| {
        let fg = fg.as_ref().unwrap();
        let (c, err) = fg.process_blocking(&mut (), || unsafe {
            // Clippy FP, does not want to move w otherwise.
            #[allow(clippy::redundant_locals)]
            let w = w;
            tlf::wgetch(w.0)
        });

        if let Some(err) = err {
            panic!("Recv error: {:?}", err);
        }
        c
    })
}

#[no_mangle]
pub unsafe extern "C" fn getnstr_process(buffer: *mut c_char, n: c_int) -> c_int {
    let buffer = AssertSend(buffer);
    FOREGROUND_WORKER.with_borrow(|fg| {
        let fg = fg.as_ref().unwrap();
        let (c, err) = fg.process_blocking(&mut (), || {
            #[allow(clippy::redundant_locals)]
            let buffer = buffer;
            unsafe { tlf::getnstr(buffer.0, n) }
        });

        if let Some(err) = err {
            panic!("Recv error: {:?}", err);
        }
        c
    })
}

pub(crate) fn exec_foreground<F: FnOnce() + Send + 'static>(f: F) {
    if in_foreground() {
        f()
    } else {
        with_foreground(|fg| fg.schedule_nowait(|_| f()).expect("send error"))
    }
}

pub(crate) fn in_foreground() -> bool {
    FOREGROUND_WORKER.with_borrow(|fg| fg.is_some())
}

pub(crate) fn with_foreground<F: FnOnce(NoWaitWorkSender<'_, ForegroundContext>) -> T, T>(
    f: F,
) -> T {
    FOREGROUND_HANDLE.with_borrow(|fg| {
        let fg = NoWaitWorkSender::new(fg.as_ref().expect("called from wrong thread"));
        f(fg)
    })
}
