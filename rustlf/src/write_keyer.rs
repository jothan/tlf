use core::panic;
use std::ffi::c_uint;

use std::os::unix::ffi::OsStrExt;
use std::sync::Arc;
use std::{
    borrow::Cow,
    ffi::{c_char, CStr, CString, OsStr},
    io::Write,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use bbqueue::{BBBuffer, Consumer, Producer};

use crate::err_utils::{log_message, LogLevel};
use crate::hamlib::Rig;
use crate::keyer_interface::CwKeyerBackend;
use crate::mfj1278::Mfj1278Keyer;
use crate::netkeyer::Netkeyer;

const KEYER_QUEUE_SIZE: usize = 400;

static KEYER_QUEUE: BBBuffer<KEYER_QUEUE_SIZE> = BBBuffer::new();
static KEYER_PRODUCER: Mutex<Option<KeyerProducer>> = Mutex::new(None);

static KEYER_FLUSH_REQUEST: AtomicBool = AtomicBool::new(false);

type KeyerProducer = Producer<'static, KEYER_QUEUE_SIZE>;
pub(crate) type KeyerConsumer = Consumer<'static, KEYER_QUEUE_SIZE>;

pub(crate) fn keyer_queue_init() -> KeyerConsumer {
    let (producer, consumer) = KEYER_QUEUE
        .try_split()
        .expect("Keyer queue initialization error");

    let mut fg_producer = KEYER_PRODUCER.lock().unwrap();
    *fg_producer = Some(producer);

    consumer
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

    while !text.is_empty() {
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
    if right.is_empty() {
        Cow::Borrowed(left)
    } else if left.is_empty() {
        Cow::Borrowed(right)
    } else {
        let mut out = left.to_owned();
        out.extend_from_slice(right);
        Cow::Owned(out)
    }
}

pub(crate) fn write_keyer(
    consumer: &mut KeyerConsumer,
    rig: Option<&mut Rig>,
    netkeyer: Option<&mut Arc<Netkeyer>>,
) {
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

    keyer_dispatch(data, rig, netkeyer);
}

fn choose_keyer<'a, 'b: 'a>(
    rig: Option<&'b mut Rig>,
    netkeyer: Option<&'b mut Arc<Netkeyer>>,
    mfj: &'b mut Mfj1278Keyer,
) -> &'a mut dyn CwKeyerBackend {
    match unsafe { tlf::cwkeyer } as c_uint {
        tlf::HAMLIB_KEYER => rig.expect("no rig when needed"),
        tlf::NET_KEYER => netkeyer.expect("no netkeyer when needed"),
        tlf::MFJ1278_KEYER => mfj,
        _ => panic!("Invalid CW keyer"),
    }
}

#[inline]
fn keyer_dispatch(data: CString, rig: Option<&mut Rig>, netkeyer: Option<&mut Arc<Netkeyer>>) {
    let trxmode = unsafe { tlf::trxmode } as u32;
    let digikeyer = unsafe { tlf::digikeyer } as u32;

    if trxmode == tlf::CWMODE {
        let mut mfj = Mfj1278Keyer;
        let keyer = choose_keyer(rig, netkeyer, &mut mfj);
        let mut data = data.into_bytes();
        keyer.prepare_message(&mut data);

        if keyer.send_message(data).is_err() {
            log_message!(LogLevel::INFO, "CW send error");
        }
    } else if trxmode == tlf::DIGIMODE {
        match digikeyer {
            tlf::FLDIGI => {
                unsafe { tlf::fldigi_send_text(data.as_ptr()) };
            }
            tlf::MFJ1278_KEYER => {
                let _ = Mfj1278Keyer.send_message(data.into_bytes());
            }
            tlf::GMFSK => {
                let path = unsafe { CStr::from_ptr(&tlf::rttyoutput as *const i8) };
                let path = OsStr::from_bytes(path.to_bytes());

                if path.is_empty() {
                    log_message!(LogLevel::WARN, "No modem file specified!");
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
            _ => panic!("Invalid digi keyer"),
        }
    }
}
