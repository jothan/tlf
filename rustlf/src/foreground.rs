use std::ffi::c_void;
use std::sync::Arc;

use crate::netkeyer::Netkeyer;
use crate::{background_process::BackgroundConfig, write_keyer::keyer_queue_init};

#[no_mangle]
pub extern "C" fn foreground_init() -> *mut c_void {
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
    });
    Box::into_raw(bg_config) as *mut c_void
}
