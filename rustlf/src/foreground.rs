use std::borrow::BorrowMut;
use std::cell::RefCell;
use std::ffi::{c_char, c_void};
use std::sync::Arc;

use crate::background_process::BackgroundContext;
use crate::err_utils::{showmsg, shownr};
use crate::hamlib::{Error, Rig, RigConfig};
use crate::netkeyer::Netkeyer;
use crate::workqueue::{workqueue, WorkSender};
use crate::{background_process::BackgroundConfig, write_keyer::keyer_queue_init};

const BACKGROUND_QUEUE_SIZE: usize = 16;

thread_local! {
    pub(crate) static BACKGROUND_HANDLE: RefCell<Option<WorkSender<BackgroundContext>>> = RefCell::new(None);
}

#[no_mangle]
pub extern "C" fn foreground_init() -> *mut c_void {
    let (bg_producer, bg_worker) = workqueue::<BackgroundContext>(BACKGROUND_QUEUE_SIZE);
    BACKGROUND_HANDLE.with(|bg| *bg.borrow_mut() = Some(bg_producer));

    let rig = unsafe { hamlib_init().ok() };

    let keyer_consumer = keyer_queue_init();

    let netkeyer = if (unsafe { tlf::cwkeyer } == tlf::NET_KEYER as _) {
        Some(unsafe { Netkeyer::from_globals() }.expect("netkeyer init error"))
    } else {
        None
    };
    let netkeyer = Arc::new(netkeyer);

    crate::netkeyer::NETKEYER.with(|fg_netkeyer| {
        *fg_netkeyer.borrow_mut() = netkeyer.clone();
    });

    fn assert_send<T: Send>() {}
    let _ = assert_send::<BackgroundConfig>;
    let bg_config = Box::new(BackgroundConfig {
        keyer_consumer,
        netkeyer,
        worker: bg_worker,
        rig,
    });
    Box::into_raw(bg_config) as *mut c_void
}

unsafe fn hamlib_init() -> Result<Rig, Error> {
    tlf::rig_set_debug(tlf::rig_debug_level_e_RIG_DEBUG_NONE);

    if !tlf::trx_control {
        return Err(Error::InvalidRigconf);
    }

    shownr!("Rig model number is", tlf::myrig_model);
    shownr!("Rig speed is", tlf::serial_rate);

    showmsg!("Trying to start rig control");

    let rig_result = RigConfig::from_globals().and_then(|config| config.open_rig());

    if rig_result.is_err() {
        showmsg!("Continue without rig control Y/(N)?");
        if (tlf::key_get() as u8).to_ascii_uppercase() != b'Y' {
            tlf::endwin();
            std::process::exit(1);
        }
        tlf::trx_control = false;
        showmsg!("Disabling rig control!");
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    rig_result
}
