use std::{ffi::{c_char, c_void, CStr, CString}, sync::Arc};

use crate::{netkeyer::{NETKEYER, Netkeyer}, foreground::BACKGROUND_HANDLE, workqueue::WorkSender, background_process::BackgroundContext};

struct PlaySoundConfig {
    pub(crate) netkeyer: Option<Arc<Netkeyer>>,
    pub(crate) bg_thread: Option<WorkSender<BackgroundContext>>,
    pub(crate) audiofile: CString,
}

#[no_mangle]
pub unsafe extern "C" fn prepare_playsound(audiofile: *const c_char) -> *mut c_void {
    let netkeyer = NETKEYER.with_borrow(|fg_netkeyer| fg_netkeyer.clone());
    let bg_thread = BACKGROUND_HANDLE.with_borrow(|bg_thread| bg_thread.clone());

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
    NETKEYER.with_borrow_mut(|audio_netkeyer| *audio_netkeyer = netkeyer);
    BACKGROUND_HANDLE.with_borrow_mut(|audio_bg| *audio_bg = bg_thread);

    audiofile.into_raw()
}

#[no_mangle]
pub unsafe extern "C" fn close_playsound(audiofile: *mut c_char) {
    std::mem::drop(CString::from_raw(audiofile));
}
