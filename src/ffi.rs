#![allow(dead_code)]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/ffi.rs"));

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct MPVHandle(pub *mut mpv_handle);
unsafe impl Send for MPVHandle {}
unsafe impl Sync for MPVHandle {}

impl From<MPVHandle> for *mut mpv_handle {
    #[inline]
    fn from(value: MPVHandle) -> Self {
        value.0
    }
}

#[repr(transparent)]
pub struct Error(pub mpv_error);

impl std::error::Error for Error {}

impl From<mpv_error> for Error {
    #[inline]
    fn from(value: mpv_error) -> Self {
        Self(value)
    }
}

impl From<Error> for zbus::fdo::Error {
    #[inline]
    fn from(value: Error) -> Self {
        Self::Failed(value.to_string())
    }
}

impl From<Error> for zbus::Error {
    #[inline]
    fn from(value: Error) -> Self {
        Self::Failure(value.to_string())
    }
}

impl std::fmt::Debug for Error {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Error(")?;
        std::fmt::Display::fmt(self, f)?;
        f.write_str(")")
    }
}

impl std::fmt::Display for Error {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = unsafe { std::ffi::CStr::from_ptr(mpv_error_string(self.0)) }
            .to_str()
            .unwrap_or_default();
        f.write_str(str)
    }
}

pub unsafe fn string_from_cstr(
    ptr: *const std::ffi::c_char,
) -> Result<String, std::str::Utf8Error> {
    std::ffi::CStr::from_ptr(ptr).to_str().map(str::to_owned)
}

pub unsafe fn string_from_cstr_lossy(ptr: *const std::ffi::c_char) -> String {
    std::ffi::CStr::from_ptr(ptr).to_string_lossy().into()
}

pub trait AsBytes {
    fn as_bytes(&self) -> &[u8];
}

impl AsBytes for &[u8] {
    #[inline]
    fn as_bytes(&self) -> &[u8] {
        self
    }
}

impl AsBytes for str {
    #[inline]
    fn as_bytes(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl AsBytes for std::ffi::CStr {
    #[inline]
    fn as_bytes(&self) -> &[u8] {
        self.to_bytes_with_nul()
    }
}
