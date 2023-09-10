#![allow(dead_code)]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use thiserror::Error;

include!(concat!(env!("OUT_DIR"), "/mpv.rs"));

pub const MPV_MPRIS: u64 = u64::from_ne_bytes(*b"mpvmpris");

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
        let str = unsafe { std::ffi::CStr::from_ptr(mpv_error_string(self.0 as _)) }
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

impl<'a> Str<'a> {
    #[inline]
    pub fn into_str(self) -> &'a str {
        self.0
    }
}

impl<'a> std::ops::Deref for Str<'a> {
    type Target = str;
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<'a> From<Str<'a>> for &'a str {
    #[inline]
    fn from(value: Str<'a>) -> Self {
        value.0
    }
}

impl<'a> From<Str<'a>> for String {
    #[inline]
    fn from(value: Str<'a>) -> Self {
        Self::from(&*value)
    }
}

impl<'a> AsRef<std::ffi::OsStr> for Str<'a> {
    #[inline]
    fn as_ref(&self) -> &std::ffi::OsStr {
        self.0.as_ref()
    }
}

impl<'a> AsRef<std::path::Path> for Str<'a> {
    #[inline]
    fn as_ref(&self) -> &std::path::Path {
        self.0.as_ref()
    }
}

impl<'a> AsRef<[u8]> for Str<'a> {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl<'a> AsRef<str> for Str<'a> {
    #[inline]
    fn as_ref(&self) -> &str {
        self.0
    }
}

impl<'a> std::borrow::Borrow<str> for Str<'a> {
    fn borrow(&self) -> &str {
        self.0
    }
}

impl<'a> PartialEq<str> for Str<'a> {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.0.eq(other)
    }
}

impl<'a> PartialOrd<str> for Str<'a> {
    #[inline]
    fn partial_cmp(&self, other: &str) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(other)
    }
}

impl<'a> PartialEq<&'a str> for Str<'a> {
    #[inline]
    fn eq(&self, other: &&'a str) -> bool {
        self.0.eq(*other)
    }
}

impl<'a> PartialOrd<&'a str> for Str<'a> {
    #[inline]
    fn partial_cmp(&self, other: &&'a str) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(*other)
    }
}

impl<'a> PartialEq<String> for Str<'a> {
    #[inline]
    fn eq(&self, other: &String) -> bool {
        self.0.eq(other)
    }
}

impl<'a> PartialOrd<String> for Str<'a> {
    #[inline]
    fn partial_cmp(&self, other: &String) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(other)
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
