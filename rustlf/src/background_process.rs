use std::ffi::{c_int, c_void};
use std::sync::{Condvar, Mutex};
use std::thread::sleep;
use std::time::Duration;

use crate::tlf;
use crate::write_keyer::{KeyerConsumer, write_keyer};

struct StopFlags {
    stopped: bool,
    stop_request: bool,
}

static STOP_PROCESS: Mutex<StopFlags> = Mutex::new(StopFlags { stopped: false, stop_request: true });
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

#[no_mangle]
pub unsafe extern "C" fn background_process(keyer_consumer: *mut c_void) -> *mut c_void {
    let mut keyer_consumer = Box::from_raw(keyer_consumer as *mut KeyerConsumer);

    let mut lantimesync: c_int = 0;
    let mut fldigi_rpc_cnt: bool = false;

    loop {
        background_process_wait();

        sleep(Duration::from_millis(10));

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
        if tlf::digikeyer == tlf::FLDIGI as _ && tlf::fldigi_isenabled() && tlf::trx_control {
            if fldigi_rpc_cnt {
                tlf::fldigi_xmlrpc_get_carrier();
                tlf::fldigi_get_log_call();
                tlf::fldigi_get_log_serial_number();
            }
            fldigi_rpc_cnt = !fldigi_rpc_cnt;
        }

        if !is_background_process_stopped() {
            tlf::cqww_simulator();
            write_keyer(&mut keyer_consumer);
        }

        tlf::handle_lan_recv(&mut lantimesync);

        tlf::gettxinfo(); /* get freq info from TRX */
    }
}
