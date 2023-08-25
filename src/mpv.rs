pub(crate) mod capi {
    #![allow(dead_code)]
    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]

    include!(concat!(env!("OUT_DIR"), "/mpv.rs"));
}

#[repr(transparent)]
pub(crate) struct Handle(pub *mut capi::mpv_handle);
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

pub(crate) const REPLY_USERDATA: u64 = u64::from_ne_bytes(*b"mpvmpris");
pub(crate) use capi::mpv_format::*;

macro_rules! assert_cstr {
    ($s:expr) => {{
        let s = $s;
        debug_assert_eq!(s.as_bytes()[s.len() - 1], '\0' as u8);
        s
    }};
}
pub(crate) use assert_cstr;

macro_rules! command {
    ($ctx:expr, $($arg:expr),+ $(,)?) => {{
        use $crate::mpv::{assert_cstr,REPLY_USERDATA,capi::mpv_command_async};
        use $crate::ptr;
        let (ctx, args) = ($ctx, ptr::from_mut(&mut [$(assert_cstr!($arg).as_ptr()),+, ptr::null()]).cast());
        unsafe {
            mpv_command_async(ctx, REPLY_USERDATA, args);
        }
    }};
}
pub(crate) use command;

macro_rules! free {
    ($value:expr) => {{
        use $crate::mpv::capi::mpv_free;
        let value: *const _ = $value;
        unsafe {
            mpv_free(value.cast_mut().cast());
        }
    }};
}
pub(crate) use free;

macro_rules! get_property {
    ($ctx:expr, $prop:expr, $format:expr, $type:ty) => {{
        use $crate::mpv::{assert_cstr, capi::mpv_get_property};
        use $crate::ptr;
        let mut rtrn = <$type>::default();
        let (ctx, prop, format, data) = (
            $ctx,
            assert_cstr!($prop).as_ptr().cast(),
            $format,
            ptr::from_mut(&mut rtrn).cast(),
        );
        unsafe {
            mpv_get_property(ctx, prop, format, data);
        }
        rtrn
    }};
}
pub(crate) use get_property;

macro_rules! get_property_bool {
    ($ctx:expr, $prop:expr) => {{
        use std::ffi::c_int;
        use $crate::mpv::{get_property, MPV_FORMAT_FLAG};
        get_property!($ctx, $prop, MPV_FORMAT_FLAG, c_int) != 0
    }};
}
pub(crate) use get_property_bool;

macro_rules! get_property_int {
    ($ctx:expr, $prop:expr) => {{
        use $crate::mpv::{get_property, MPV_FORMAT_INT64};
        get_property!($ctx, $prop, MPV_FORMAT_INT64, i64)
    }};
}
pub(crate) use get_property_int;

macro_rules! get_property_float {
    ($ctx:expr, $prop:expr) => {{
        use $crate::mpv::{get_property, MPV_FORMAT_DOUBLE};
        get_property!($ctx, $prop, MPV_FORMAT_DOUBLE, f64)
    }};
}
pub(crate) use get_property_float;

macro_rules! get_property_string {
    ($ctx:expr, $prop:expr) => {{
        use std::ffi::CStr;
        use $crate::mpv::{capi::mpv_get_property_string, free};
        let (ctx, prop) = ($ctx, $prop.as_ptr().cast());
        let cstr = unsafe { mpv_get_property_string(ctx, prop) };
        if cstr.is_null() {
            ""
        } else {
            scopeguard::guard(unsafe { CStr::from_ptr(cstr) }, |v| free!(v.as_ptr()))
                .to_str()
                .unwrap_or_default()
        }
    }};
}
pub(crate) use get_property_string;

macro_rules! set_property {
    ($ctx:expr, $prop:expr, $format:expr, $data:expr) => {{
        use $crate::mpv::{assert_cstr, capi::mpv_set_property_async, REPLY_USERDATA};
        use $crate::ptr;
        let (ctx, prop, format, data) = (
            $ctx,
            assert_cstr!($prop).as_ptr().cast(),
            $format,
            ptr::from_mut(&mut $data).cast(),
        );
        unsafe {
            mpv_set_property_async(ctx, REPLY_USERDATA, prop, format, data);
        }
    }};
}
pub(crate) use set_property;

macro_rules! set_property_bool {
    ($ctx:expr, $prop:expr, $value:expr) => {{
        use std::ffi::c_int;
        use $crate::mpv::{set_property, MPV_FORMAT_FLAG};
        set_property!($ctx, $prop, MPV_FORMAT_FLAG, $value as c_int)
    }};
}
pub(crate) use set_property_bool;

macro_rules! set_property_float {
    ($ctx:expr, $prop:expr, $data:expr) => {{
        use $crate::mpv::{set_property, MPV_FORMAT_DOUBLE};
        set_property!($ctx, $prop, MPV_FORMAT_DOUBLE, $data as f64)
    }};
}
pub(crate) use set_property_float;

macro_rules! set_property_string {
    ($ctx:expr, $prop:expr, $data:expr) => {{
        use $crate::mpv::{assert_cstr, set_property, MPV_FORMAT_STRING};
        set_property!($ctx, $prop, MPV_FORMAT_STRING, assert_cstr!($data))
    }};
}
pub(crate) use set_property_string;

macro_rules! observe_property_format {
    ($ctx:expr, $prop:expr, $format:expr) => {{
        use $crate::mpv::{assert_cstr, capi::mpv_observe_property, REPLY_USERDATA};
        let (ctx, prop, format) = ($ctx, assert_cstr!($prop).as_ptr().cast(), $format);
        unsafe {
            mpv_observe_property(ctx, REPLY_USERDATA, prop, format);
        }
    }};
}
pub(crate) use observe_property_format;

macro_rules! observe_properties {
    ($ctx:expr, $($prop:expr),+ $(,)?) => {{
        use $crate::mpv::{observe_property_format, MPV_FORMAT_NONE};
        $(observe_property_format!($ctx, $prop, MPV_FORMAT_NONE));+
    }};
}
pub(crate) use observe_properties;
