/// CW Simulator
/// works only in RUN mode for CQWW contest
use cstr::cstr;
use std::{
    cmp::min,
    ffi::{c_int, CStr, CString},
    sync::OnceLock,
};

use rand::{seq::SliceRandom, Rng};

use crate::{
    background_process::{is_background_process_stopped, with_background},
    netkeyer::write_tone,
    newtlf::countryfile::ffi::DXCC_DATA,
};

static CALLMASTER_RANDOM_LIST: OnceLock<Vec<CString>> = OnceLock::new();

const CW_TONES: [c_int; 10] = [625, 800, 650, 750, 700, 725, 675, 775, 600, 640];

pub struct CqwwSimulator {
    enabled: bool,
    tone: c_int,
    tonecpy: Option<c_int>,
    current_call: Option<&'static CStr>,
    repeat_count: usize,
}

impl Default for CqwwSimulator {
    fn default() -> Self {
        Self::new()
    }
}

impl CqwwSimulator {
    pub fn new() -> Self {
        CqwwSimulator {
            enabled: false,
            tone: CW_TONES[0],
            tonecpy: None,
            current_call: None,
            repeat_count: 0,
        }
    }

    pub fn enable(&mut self) {
        CALLMASTER_RANDOM_LIST.get_or_init(|| {
            crate::newtlf::callmaster::GLOBAL_CALLMASTER
                .read()
                .unwrap()
                .as_inner()
                .iter()
                .cloned()
                .collect()
        });

        self.pick_call();
        self.enabled = true;
    }

    pub fn disable(&mut self) {
        self.enabled = false;
    }

    fn pick_call(&mut self) {
        let list = CALLMASTER_RANDOM_LIST.get().unwrap();
        self.current_call = list.choose(&mut rand::thread_rng()).map(AsRef::as_ref);
    }

    fn set_tone(&mut self) {
        // TODO: see if write_tone entry point can be removed.
        self.tonecpy = Some(unsafe { write_tone(self.tone) });

        unsafe { tlf::sendmessage(cstr!("  ").as_ptr()) };
    }

    fn restore_tone(&mut self) {
        if let Some(tonecpy) = self.tonecpy {
            unsafe { write_tone(tonecpy) };
        }
    }

    pub fn send_call(&mut self) {
        // First QSO step
        self.tone = CW_TONES.choose(&mut rand::thread_rng()).copied().unwrap();
        self.set_tone();
        self.pick_call();

        unsafe { tlf::sendmessage(self.current_call.unwrap().as_ptr()) };
        self.repeat_count = 0;
        self.restore_tone();
    }

    pub fn send_final(&mut self) {
        self.set_tone();
        let ctydata = unsafe { DXCC_DATA.get() };
        let call = self.current_call.unwrap();
        let (i, _) = ctydata.prefixes.getpfxindex(call.to_str().unwrap());
        let pdata = ctydata.prefixes.get(i.unwrap()).unwrap();

        let zone: u8 = pdata.cq_zone.into();
        let mut zone_str = format!("{zone:02}");

        // Use short numbers randomly
        if rand::thread_rng().gen_ratio(1, 2) && zone_str.starts_with('0') {
            zone_str = zone_str.replacen('0', "T", 1);
        }

        let msg = CString::new(format!("TU 5NN {zone_str}")).unwrap();
        unsafe { tlf::sendmessage(msg.as_ptr()) };
        self.repeat_count = 0;
        self.restore_tone();
    }

    pub fn send_repeat(&mut self) {
        self.set_tone();
        // God save the poor soul who overflows this.
        self.repeat_count = self.repeat_count.saturating_add(1);
        let slow = min(self.repeat_count / 2, 3);
        let mut msg = String::new();

        msg.extend(std::iter::repeat('-').take(slow));
        msg.push_str(self.current_call.unwrap().to_str().unwrap());
        msg.extend(std::iter::repeat('+').take(slow));

        let msg = CString::new(msg).unwrap();
        unsafe { tlf::sendmessage(msg.as_ptr()) };
        self.restore_tone();
    }

    fn precondition(&self) -> bool {
        self.enabled
            && !is_background_process_stopped()
            && unsafe { tlf::trxmode == tlf::CWMODE.try_into().unwrap() }
    }
}

fn with_simulator<F: FnOnce(&mut CqwwSimulator) + Send + 'static>(f: F) {
    with_background(|bg| {
        bg.schedule_nowait(|ctx| f(&mut ctx.simulator))
            .expect("background send error")
    })
}

#[no_mangle]
pub extern "C" fn simulator_enable() {
    with_simulator(|sim| sim.enable())
}

#[no_mangle]
pub extern "C" fn simulator_disable() {
    with_simulator(|sim| sim.disable())
}

#[no_mangle]
pub extern "C" fn simulator_send_call() {
    with_simulator(|sim| {
        if sim.precondition() {
            sim.send_call();
        }
    })
}

#[no_mangle]
pub extern "C" fn simulator_send_final() {
    with_simulator(|sim| {
        if sim.precondition() {
            sim.send_final();
        }
    })
}

#[no_mangle]
pub extern "C" fn simulator_send_repeat() {
    with_simulator(|sim| {
        if sim.precondition() {
            sim.send_repeat();
        }
    })
}
