#![allow(dead_code)]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/ffi.rs"));

use thiserror::Error;

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Handle(pub *mut mpv_handle);
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

impl From<Handle> for *mut mpv_handle {
    #[inline]
    fn from(value: Handle) -> Self {
        value.0
    }
}

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
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub struct Str(str);

impl Drop for Str {
    #[inline]
    fn drop(&mut self) {
        unsafe { mpv_free(self.as_ptr().cast_mut().cast()) }
    }
}

impl<'a> TryFrom<&'a std::ffi::c_char> for &'a Str {
    type Error = std::str::Utf8Error;
    fn try_from(value: &'a std::ffi::c_char) -> Result<Self, Self::Error> {
        // SAFETY: value cannot be null
        unsafe { std::ffi::CStr::from_ptr(value) }
            .to_str()
            .map(Str::from_str)
    }
}

impl std::ops::Deref for Str {
    type Target = str;
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl Default for &Str {
    #[inline]
    fn default() -> Self {
        Str::from_str(Default::default())
    }
}

impl<'a> Str {
    #[inline]
    const fn from_str(value: &'a str) -> &'a Self {
        unsafe { &*(value as *const str as *const Self) }
    }
    #[inline]
    pub const fn as_str(&'a self) -> &'a str {
        unsafe { &*(self as *const Self as *const str) }
    }
}

impl<'a> From<&'a Str> for &'a str {
    #[inline]
    fn from(value: &'a Str) -> Self {
        value.as_str()
    }
}

impl From<&Str> for String {
    #[inline]
    fn from(value: &Str) -> Self {
        value.as_str().to_owned()
    }
}

impl AsRef<str> for Str {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<std::ffi::OsStr> for Str {
    #[inline]
    fn as_ref(&self) -> &std::ffi::OsStr {
        self.as_str().as_ref()
    }
}

impl AsRef<std::path::Path> for Str {
    #[inline]
    fn as_ref(&self) -> &std::path::Path {
        self.as_str().as_ref()
    }
}

impl PartialEq<str> for Str {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.as_str().eq(other)
    }
}

impl PartialOrd<str> for Str {
    #[inline]
    fn partial_cmp(&self, other: &str) -> Option<std::cmp::Ordering> {
        self.as_str().partial_cmp(other)
    }
}

impl std::fmt::Debug for &Str {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

impl std::fmt::Display for &Str {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}
