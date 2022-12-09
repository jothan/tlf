use std::ffi::{c_char, c_uint, c_void, CStr};
use std::sync::Arc;

use crate::netkeyer::Netkeyer;
use crate::{background_process::BackgroundConfig, write_keyer::keyer_queue_init};


#[no_mangle]
pub extern "C" fn foreground_init() -> *mut c_void {
    let keyer_consumer = keyer_queue_init();
    let mut netkeyer = None;

    if (unsafe { tlf::cwkeyer } == tlf::NET_KEYER as _) {
        let host = unsafe { CStr::from_ptr(&tlf::netkeyer_hostaddress as *const c_char) };
        let port = unsafe { tlf::netkeyer_port as c_uint }.try_into().unwrap();

        netkeyer = Some(Netkeyer::from_host_and_port(
            host.to_str().expect("invalid netkeyer host string"),
            port,
        ).expect("netkeyer init error"))
    }
    if let Some(netkeyer) = netkeyer.as_ref() {
        unsafe { crate::netkeyer::init_params(netkeyer).expect("netkeyer send error") };
    }
    let netkeyer = Arc::new(netkeyer);

    crate::netkeyer::NETKEYER.with(|fg_netkeyer| {
        *fg_netkeyer.borrow_mut() = netkeyer.clone();
    });

    fn assert_send<T: Send>() {}
    let _ = assert_send::<BackgroundConfig>;
    let bg_config = Box::new(BackgroundConfig { keyer_consumer, netkeyer });
    Box::into_raw(bg_config) as *mut c_void
}

