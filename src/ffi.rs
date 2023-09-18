#![allow(dead_code)]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/ffi.rs"));

use mpris_server::zbus;
use thiserror::Error;

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Handle(pub *mut mpv_handle);
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

#[repr(transparent)]
#[derive(Error)]
pub struct Error(pub mpv_error);

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

#[repr(transparent)]
#[derive(Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Str<'a>(pub &'a str);

impl<'a> TryFrom<&'a std::ffi::c_char> for Str<'a> {
    type Error = std::str::Utf8Error;
    fn try_from(value: &'a std::ffi::c_char) -> Result<Self, Self::Error> {
        // SAFETY: value cannot be null
        unsafe { std::ffi::CStr::from_ptr(value) }
            .to_str()
            .map(Self)
    }
}

impl<'a> Drop for Str<'a> {
    fn drop(&mut self) {
        unsafe { mpv_free(self.0.as_ptr().cast_mut().cast()) }
    }
}

impl<'a> std::ops::Deref for Str<'a> {
    type Target = str;
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<'a> std::fmt::Debug for Str<'a> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<'a> std::fmt::Display for Str<'a> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
