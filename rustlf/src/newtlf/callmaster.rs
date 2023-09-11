use std::{
    collections::BTreeSet,
    ffi::{c_char, CStr, CString},
    io::Read,
    ops::RangeFrom,
    sync::RwLock,
};

use libc::c_void;
use linereader::LineReader;

pub struct CallMaster(BTreeSet<CString>);

impl CallMaster {
    pub fn parse<R: Read, C: FnMut(&str)>(
        reader: R,
        max_line_length: usize,
        mut consumer: C,
    ) -> Result<(), std::io::Error> {
        let mut reader = LineReader::with_capacity(max_line_length, reader);

        while let Some(input_line) = reader.next_line() {
            let input_line = String::from_utf8_lossy(input_line?);
            let input_line = input_line.trim();

            if input_line.starts_with('#') || input_line.len() < 3 {
                continue;
            }
            consumer(input_line);
        }

        Ok(())
    }

    pub fn load<R: Read>(
        reader: R,
        max_line_length: usize,
        only_na: bool,
    ) -> Result<Self, std::io::Error> {
        let mut set = BTreeSet::new();

        Self::parse(reader, max_line_length, |call| {
            if only_na && !"AKWVCN".contains(call.chars().next().unwrap()) {
                return;
            }
            let mut call = call.to_owned();
            call.make_ascii_uppercase();
            if let Ok(call) = CString::new(call) {
                set.insert(call);
            }
        })?;

        Ok(CallMaster(set))
    }

    pub fn starting_with<'a>(&'a self, query: &'a CString) -> impl Iterator<Item = &CStr> + 'a {
        // FIXME: find a way to feed a CStr to BTreeSet::range.
        self.0
            .range::<CString, RangeFrom<&CString>>(query..)
            .take_while(|&call| call.as_bytes().starts_with(query.to_bytes()))
            .map(CString::as_c_str)
    }

    pub fn containing<'a>(&'a self, query: &'a CStr) -> impl Iterator<Item = &CStr> + 'a {
        let query = query.to_string_lossy();

        self.0
            .iter()
            .filter(move |&call| {
                // Safety: all set calls must be valid UTF-8.
                let call = unsafe { std::str::from_utf8_unchecked(call.as_bytes()) };
                call.contains(query.as_ref())
            })
            .map(CString::as_c_str)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_inner(&self) -> &BTreeSet<CString> {
        &self.0
    }
}

pub static GLOBAL_CALLMASTER: RwLock<CallMaster> = RwLock::new(CallMaster(BTreeSet::new()));

type ShowPartialFn = extern "C" fn(*const c_char, *const c_void) -> bool;

#[no_mangle]
pub unsafe extern "C" fn callmaster_show_partials(
    query: *const c_char,
    callback: ShowPartialFn,
    callback_arg: *const c_void,
) {
    let query: CString = CStr::from_ptr(query).into();

    let guard = GLOBAL_CALLMASTER.read().unwrap();
    let iter = guard.starting_with(&query).chain(guard.containing(&query));

    for call in iter {
        if callback(call.as_ptr(), callback_arg) {
            break;
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn callmaster_contains(query: *const c_char) -> bool {
    let query = CStr::from_ptr(query);
    let guard = GLOBAL_CALLMASTER.read().unwrap();
    guard.0.contains(query)
}

#[no_mangle]
pub extern "C" fn callmaster_len() -> usize {
    let guard = GLOBAL_CALLMASTER.read().unwrap();
    guard.0.len()
}

pub const CALLMASTER_VERSION_LEN: usize = 11;

/// cbindgen:ptrs-as-arrays=[[buffer; CALLMASTER_VERSION_LEN+1]]
#[no_mangle]
pub unsafe extern "C" fn callmaster_version(buffer: *mut c_char) {
    let guard = GLOBAL_CALLMASTER.read().unwrap();
    let query = CString::new("VER").unwrap();
    let version = guard
        .starting_with(&query)
        .find(|c| c.to_bytes().len() == CALLMASTER_VERSION_LEN);

    if let Some(version) = version {
        buffer.copy_from_nonoverlapping(version.as_ptr(), CALLMASTER_VERSION_LEN + 1);
    } else {
        buffer.write(0);
    }
}
