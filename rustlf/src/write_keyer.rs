use std::{
    borrow::Cow,
    ffi::{c_char, c_int, c_void, CStr, CString, OsStr},
    io::Write,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use std::os::unix::ffi::OsStrExt;

use bbqueue::{BBBuffer, Consumer, Producer};

use crate::err_utils::{log_message, LogLevel};
use crate::tlf;

const KEYER_QUEUE_SIZE: usize = 400;

static KEYER_QUEUE: BBBuffer<KEYER_QUEUE_SIZE> = BBBuffer::new();
static KEYER_PRODUCER: Mutex<Option<KeyerProducer>> = Mutex::new(None);

static KEYER_FLUSH_REQUEST: AtomicBool = AtomicBool::new(false);

type KeyerProducer = Producer<'static, KEYER_QUEUE_SIZE>;
pub(crate) type KeyerConsumer = Consumer<'static, KEYER_QUEUE_SIZE>;

#[no_mangle]
pub extern "C" fn keyer_queue_init() -> *mut c_void {
    let (producer, consumer) = KEYER_QUEUE
        .try_split()
        .expect("Keyer queue initialization error");

    {
        let mut fg_producer = KEYER_PRODUCER.lock().unwrap();
        *fg_producer = Some(producer);
    }

    let bg_consumer: Box<KeyerConsumer> = Box::new(consumer);
    let bg_consumer: *mut KeyerConsumer = Box::into_raw(bg_consumer);
    bg_consumer as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn keyer_append(text: *const c_char) {
    let text = CStr::from_ptr(text).to_bytes();
    keyer_append_safe(text);
}

#[inline]
fn keyer_append_safe(mut text: &[u8]) {
    let mut producer = KEYER_PRODUCER.lock().unwrap();
    let producer = producer.as_mut().expect("Keyer queue not initialized");

    while text.len() != 0 {
        let mut grant = match producer.grant_max_remaining(text.len()) {
            Ok(grant) => grant,
            Err(bbqueue::Error::InsufficientSize) => return, // Overflow, ignore for now
            Err(_) => panic!("Keyer queue error"),
        };

        let buf = grant.buf();

        buf.copy_from_slice(&text[..buf.len()]);
        (_, text) = text.split_at(buf.len());

        let len = buf.len();
        grant.commit(len);
    }
}

#[no_mangle]
pub extern "C" fn keyer_append_char(c: c_char) {
    keyer_append_safe(std::slice::from_ref(&(c as u8)));
}

#[no_mangle]
pub extern "C" fn keyer_flush() {
    KEYER_FLUSH_REQUEST.store(true, Ordering::SeqCst);
}

fn combine_segments<'a>((left, right): (&'a [u8], &'a [u8])) -> Cow<'a, [u8]> {
    if right.len() == 0 {
        Cow::Borrowed(left)
    } else if left.len() == 0 {
        Cow::Borrowed(right)
    } else {
        let mut out = left.to_owned();
        out.extend_from_slice(right);
        Cow::Owned(out)
    }
}

pub(crate) fn write_keyer(consumer: &mut KeyerConsumer) {
    let trxmode = unsafe { tlf::trxmode } as u32;
    if trxmode != tlf::CWMODE && trxmode != tlf::DIGIMODE {
        return;
    }

    // Consume flush no matter what.
    let do_flush = KEYER_FLUSH_REQUEST.swap(false, Ordering::SeqCst);

    let grant = match consumer.split_read() {
        Err(bbqueue::Error::InsufficientSize) => return,
        Err(_) => panic!("Keyer write error"),
        Ok(g) => g,
    };

    let len = grant.combined_len();
    if do_flush {
        grant.release(len);
        return;
    }

    let data =
        CString::new(combine_segments(grant.bufs())).expect("Unexpected 0 byte in keyer data");
    grant.release(len);

    keyer_dispatch(data);
}

#[inline]
fn keyer_dispatch(data: CString) {
    let trxmode = unsafe { tlf::trxmode } as u32;
    let cwkeyer = unsafe { tlf::cwkeyer } as u32;
    let digikeyer = unsafe { tlf::digikeyer } as u32;

    if digikeyer == tlf::FLDIGI && trxmode == tlf::DIGIMODE {
        unsafe { tlf::fldigi_send_text(data.as_ptr()) };
    } else if cwkeyer == tlf::NET_KEYER {
        unsafe { tlf::netkeyer(tlf::K_MESSAGE as i32, data.as_ptr()) };
    } else if cwkeyer == tlf::HAMLIB_KEYER {
        let mut data_bytes = data.into_bytes();
        // Filter out unsupported speed directives
        data_bytes.retain(|c| *c != b'+' && *c != b'-');

        let data = CString::new(data_bytes).unwrap();
        let error = unsafe { tlf::hamlib_keyer_send(data.as_ptr()) };
        if error != tlf::rig_errcode_e_RIG_OK as i32 {
            let str_error = unsafe { tlf::rigerror(error) };
            let str_error = unsafe { CStr::from_ptr(str_error) }.to_string_lossy();
            let str_error = CString::new(format!("CW send error: {}", str_error)).unwrap();
            log_message(LogLevel::WARN, str_error);
        }
    } else if cwkeyer == tlf::MFJ1278_KEYER || digikeyer == tlf::MFJ1278_KEYER {
        let path = unsafe { CStr::from_ptr(&tlf::controllerport as *const i8) }.to_string_lossy();
        let file_open = std::fs::File::options()
            .append(true)
            .create(false)
            .open(path.as_ref());
        match file_open {
            Ok(mut file) => {
                // FIXME: should this be silent ?
                let _ = file.write_all(data.as_bytes());
            }
            Err(_) => {
                log_message(
                    LogLevel::WARN,
                    CString::new("1278 not active. Switching to SSB mode.").unwrap(),
                );
                unsafe {
                    tlf::trxmode = tlf::SSBMODE as c_int;
                    tlf::clear_display();
                }
            }
        }
    } else if digikeyer == tlf::GMFSK {
        let path = unsafe { CStr::from_ptr(&tlf::rttyoutput as *const i8) };
        let path = OsStr::from_bytes(path.to_bytes());

        if path.is_empty() {
            log_message(
                LogLevel::WARN,
                CString::new("No modem file specified!").unwrap(),
            );
        }

        let mut data_bytes = data.into_bytes();
        if data_bytes.last() == Some(&b'\n') {
            data_bytes.pop();
        }
        data_bytes.insert(0, b'\n');

        // FIXME: original code seems to want to fire this asynchronously and forget about it.
        let file_open = std::fs::File::options()
            .append(true)
            .create(false)
            .open(path);

        if let Ok(mut file) = file_open {
            let _ = file.write_all(&data_bytes);
        }
    }
}
