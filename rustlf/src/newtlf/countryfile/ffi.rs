use std::{
    cell::UnsafeCell,
    ffi::{c_char, CStr},
    fs::File,
    str::Utf8Error,
};

use crate::err_utils::CResult;

use super::{dummy_country, dummy_prefix, Country, DxccData, Prefix};

// Safety: calling code expected to enforce synchronization
struct GlobalDxccData(std::cell::UnsafeCell<Option<DxccData>>);

unsafe impl Sync for GlobalDxccData {}

static DXCC_DATA: GlobalDxccData = GlobalDxccData(UnsafeCell::new(None));

impl GlobalDxccData {
    unsafe fn get(&self) -> &DxccData {
        let inner = &mut *self.0.get();
        inner.as_ref().expect("GlobalDxccData not initialized")
    }

    unsafe fn get_mut(&self) -> &mut DxccData {
        let inner = &mut *self.0.get();
        inner.get_or_insert_with(Default::default)
    }
}

unsafe fn ptr_to_str<'a>(s: *const c_char) -> Result<&'a str, Utf8Error> {
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
pub unsafe extern "C" fn load_ctydata(path: *const c_char) -> CResult {
    let dd = unsafe { DXCC_DATA.get_mut() };
    let path = unsafe { ptr_to_str(path).map_err(|_| std::io::ErrorKind::InvalidData.into()) };

    path.and_then(File::open).and_then(|file| {
        DxccData::load::<std::io::Error, _>(file)
    }).map(|data| {
        *dd = data; Ok::<_, std::io::Error>(())
    }).into()
}

