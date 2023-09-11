use std::{
    cell::UnsafeCell,
    ffi::{c_char, c_int, CStr, CString},
    fs::File,
    str::Utf8Error,
    sync::OnceLock,
};

use super::{dummy_country, dummy_prefix, Country, DxccData, Prefix};

// Safety: calling code expected to enforce synchronization
pub struct GlobalDxccData(std::cell::UnsafeCell<Option<DxccData>>);

unsafe impl Sync for GlobalDxccData {}

pub static DXCC_DATA: GlobalDxccData = GlobalDxccData(UnsafeCell::new(None));

impl GlobalDxccData {
    pub unsafe fn get(&self) -> &DxccData {
        let inner = &mut *self.0.get();
        inner.as_ref().expect("GlobalDxccData not initialized")
    }

    #[allow(clippy::mut_from_ref)]
    pub unsafe fn get_mut(&self) -> &mut DxccData {
        let inner = &mut *self.0.get();
        inner.get_or_insert_with(Default::default)
    }
}

unsafe fn ptr_to_str<'a>(s: *const c_char) -> Result<&'a str, Utf8Error> {
    assert!(!s.is_null());
    CStr::from_ptr(s).to_str()
}

#[allow(non_camel_case_types)]
pub type dxcc_data = Country;

#[allow(non_camel_case_types)]
pub type prefix_data = Prefix;

#[no_mangle]
pub extern "C" fn dxcc_by_index(mut index: usize) -> *const dxcc_data {
    let dd = unsafe { DXCC_DATA.get() };

    if index >= dd.countries.len() {
        index = 0;
    }
    dd.countries.get(index).unwrap_or_else(|| dummy_country())
}

#[no_mangle]
pub extern "C" fn prefix_by_index(index: usize) -> *const prefix_data {
    let dd = unsafe { DXCC_DATA.get() };
    dd.prefixes.get(index).unwrap_or_else(|| dummy_prefix())
}

#[no_mangle]
pub extern "C" fn dxcc_count() -> usize {
    let dd = unsafe { DXCC_DATA.get() };
    dd.countries.len()
}

#[no_mangle]
pub extern "C" fn prefix_count() -> usize {
    let dd = unsafe { DXCC_DATA.get() };
    dd.prefixes.len()
}

#[no_mangle]
pub unsafe extern "C" fn find_full_match(call: *const c_char) -> isize {
    let dd = unsafe { DXCC_DATA.get() };
    let call = unsafe { ptr_to_str(call).ok() };
    call.and_then(|call| dd.prefixes.find_full_match(call))
        .and_then(|idx| idx.try_into().ok())
        .unwrap_or(-1)
}

#[no_mangle]
pub unsafe extern "C" fn find_best_match(call: *const c_char) -> isize {
    let dd = unsafe { DXCC_DATA.get() };
    let call = unsafe { ptr_to_str(call).ok() };
    call.and_then(|call| dd.prefixes.find_best_match(call))
        .and_then(|idx| idx.try_into().ok())
        .unwrap_or(-1)
}

/* prepare and check callsign and look it up in dxcc data base
 *
 * returns index in data base or -1 if not found
 * if normalized_call ptr is not NULL returns a copy of the normalized call
 * e.g. DL1XYZ/PA gives PA/DL1XYZ
 * caller has to free the copy after use
 */
#[no_mangle]
pub unsafe extern "C" fn getpfxindex(
    call: *const c_char,
    normalized_call: *mut *mut c_char,
) -> isize {
    if call.is_null() {
        return -1;
    }
    let dd = unsafe { DXCC_DATA.get() };
    let call = if let Ok(call) = unsafe { ptr_to_str(call) } {
        call
    } else {
        return -1;
    };

    let (idx, normalized) = dd.prefixes.getpfxindex(call);

    if !normalized_call.is_null() {
        let normalized = CString::new(normalized.into_owned()).unwrap();
        unsafe { *normalized_call = libc::strdup(normalized.as_ptr()) };
    }

    idx.and_then(|idx| idx.try_into().ok()).unwrap_or(-1)
}

static GETCTYNR_MOCK: OnceLock<usize> = OnceLock::new();

#[no_mangle]
pub unsafe extern "C" fn mock_getctynr(idx: usize) {
    let _ = GETCTYNR_MOCK.set(idx);
    DXCC_DATA.get_mut();
}

fn special_getctynr(call: *const c_char) -> usize {
    let call = unsafe { ptr_to_str(call).unwrap() };

    // used for "PFX_NUM_MULTIS=W,VE,VK,ZL,ZS,JA,PY,UA9"
    if call.starts_with('W') {
        return 18;
    }
    if call.starts_with("VE") {
        return 17;
    }
    if call.starts_with("VK") {
        return 16;
    }
    if call.starts_with("ZL") {
        return 15;
    }
    if call.starts_with("ZS") {
        return 14;
    }
    if call.starts_with("JA") {
        return 13;
    }
    if call.starts_with("PY") {
        return 12;
    }
    if call.starts_with("UA9") {
        return 11;
    }

    // used for COUNTRYLIST
    if call.starts_with("GM") {
        return 100;
    }
    if call.starts_with("HG") {
        return 101;
    }
    if call.starts_with("EA") {
        return 102;
    }
    if call.starts_with("EB") {
        return 102;
    }

    0
}

/// Lookup dxcc cty number from callsign
#[no_mangle]
pub unsafe extern "C" fn getctynr(call: *const c_char) -> usize {
    if let Some(idx) = GETCTYNR_MOCK.get() {
        if *idx == 99 {
            return special_getctynr(call);
        }
        return *idx;
    }
    let dd = unsafe { DXCC_DATA.get() };
    let call = if let Ok(call) = unsafe { ptr_to_str(call) } {
        call
    } else {
        return 0;
    };

    let (idx, _) = dd.prefixes.getpfxindex(call);
    idx.and_then(|idx| dd.prefixes.get(idx))
        .map(|prefix| prefix.country_idx)
        .unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "C" fn getctyinfo(call: *const c_char) -> *const prefix_data {
    if GETCTYNR_MOCK.get().is_some() {
        return std::ptr::null();
    }

    if call.is_null() {
        return dummy_prefix();
    }

    let dd = unsafe { DXCC_DATA.get() };
    let call = unsafe { ptr_to_str(call).unwrap() };
    let (idx, _) = dd.prefixes.getpfxindex(call);
    idx.and_then(|idx| dd.prefixes.get(idx))
        .unwrap_or_else(|| dummy_prefix())
}

#[no_mangle]
pub extern "C" fn cty_dat_version() -> *const c_char {
    let dd = unsafe { DXCC_DATA.get() };
    dd.prefixes.version.as_slice().as_ptr() as *const c_char
}

#[no_mangle]
pub extern "C" fn dxcc_init() {
    let dd = unsafe { DXCC_DATA.get_mut() };
    dd.countries.clear();
}

#[no_mangle]
pub extern "C" fn prefix_init() {
    let dd = unsafe { DXCC_DATA.get_mut() };
    dd.prefixes.clear();
}

#[no_mangle]
pub unsafe extern "C" fn dxcc_add(line: *const c_char) {
    let dd = unsafe { DXCC_DATA.get_mut() };
    let line = unsafe { ptr_to_str(line).unwrap() };

    dd.push_country_str(line).expect("invalid country line");
}

#[no_mangle]
pub unsafe extern "C" fn prefix_add(line: *const c_char) {
    let dd = unsafe { DXCC_DATA.get_mut() };
    let line = unsafe { ptr_to_str(line).unwrap() };

    dd.push_prefix_str(line).expect("invalid prefix line");
}

#[no_mangle]
pub unsafe extern "C" fn load_ctydata(path: *const c_char) -> c_int {
    let dd = unsafe { DXCC_DATA.get_mut() };
    let path = unsafe { ptr_to_str(path).map_err(|_| std::io::ErrorKind::InvalidData.into()) };

    match path
        .and_then(File::open)
        .and_then(DxccData::load::<std::io::Error, _>)
    {
        Ok(data) => {
            *dd = data;
            0
        }
        Err(_) => -1,
    }
}
