use std::{ffi::{c_char, CStr, CString}, str::Utf8Error};

/// An owned C string that can be accessed on the C side as a plain pointer.
#[repr(transparent)]
pub struct CStringPtr(*mut c_char);

unsafe impl Send for CStringPtr {}
unsafe impl Sync for CStringPtr {}

impl std::fmt::Debug for CStringPtr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().unwrap().fmt(f)
    }
}

impl CStringPtr {
    pub fn as_cstr(&self) -> &CStr {
        unsafe { CStr::from_ptr(self.0) }
    }

    pub fn as_str(&self) -> Result<&str, Utf8Error> {
        self.as_cstr().to_str()
    }
}

impl From<CString> for CStringPtr {
    fn from(s: CString) -> Self {
        Self(s.into_raw())
    }
}

impl Drop for CStringPtr {
    fn drop(&mut self) {
        let _ = unsafe { CString::from_raw(self.0) };
    }
}

/// A C string constant that can be accessed on the C side as a plain pointer.
#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct StaticCStrPtr(*const c_char);

unsafe impl Send for StaticCStrPtr {}
unsafe impl Sync for StaticCStrPtr {}

impl From<&'static CStr> for StaticCStrPtr {
    fn from(s: &'static CStr) -> Self {
        Self(s.as_ptr())
    }
}

impl std::fmt::Debug for StaticCStrPtr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = unsafe { CStr::from_ptr(self.0) };
        s.to_str().unwrap().fmt(f)
    }
}

