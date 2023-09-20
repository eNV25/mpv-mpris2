#![macro_use]

macro_rules! cstr {
    ($s:literal) => {
        concat!($s, "\0").as_ptr().cast::<std::ffi::c_char>()
    };
    ($s:expr) => {{
        debug_assert_eq!($s.as_bytes().last(), Some(&b'\0'));
        $s.as_ptr().cast::<std::ffi::c_char>()
    }};
}

macro_rules! command {
    ($ctx:ident, $($arg:expr),+ $(,)?) => {{
        let args = [$(cstr!($arg)),+, std::ptr::null()];
        match unsafe { $crate::mpv_command($ctx.into(), std::ptr::addr_of!(args).cast_mut().cast()) } {
            0.. => Ok(()),
            error => Err($crate::Error(error.into())),
        }
    }};
}

macro_rules! get {
    ($ctx:ident, $prop:literal) => {
        get!($ctx, $prop, MPV_FORMAT_STRING)
    };
    ($ctx:ident, $prop:literal, bool) => {
        get!($ctx, $prop, MPV_FORMAT_FLAG).map(|x| x != 0)
    };
    ($ctx:ident, $prop:literal, i64) => {
        get!($ctx, $prop, MPV_FORMAT_INT64)
    };
    ($ctx:ident, $prop:literal, f64) => {
        get!($ctx, $prop, MPV_FORMAT_DOUBLE)
    };
    ($ctx:ident, $prop:literal, MPV_FORMAT_STRING) => {
        unsafe { $crate::mpv_get_property_string($ctx.into(), cstr!($prop)).as_ref() }
            .and_then(|s| $crate::Str::try_from(s).ok())
            .map(|s| String::from(&*s))
    };
    ($ctx:ident, $prop:literal, MPV_FORMAT_OSD_STRING) => {
        get!(
            $ctx,
            $prop,
            MPV_FORMAT_OSD_STRING,
            std::ptr::null::<std::ffi::c_char>()
        )
        .ok()
        .and_then(|s| unsafe { s.as_ref() })
        .and_then(|s| $crate::Str::try_from(s).ok())
        .map(String::from)
    };
    ($ctx:ident, $prop:literal, MPV_FORMAT_FLAG) => {
        get!($ctx, $prop, MPV_FORMAT_FLAG, std::ffi::c_int::default())
    };
    ($ctx:ident, $prop:literal, MPV_FORMAT_INT64) => {
        get!($ctx, $prop, MPV_FORMAT_INT64, i64::default())
    };
    ($ctx:ident, $prop:literal, MPV_FORMAT_DOUBLE) => {
        get!($ctx, $prop, MPV_FORMAT_DOUBLE, f64::default())
    };
    ($ctx:ident, $prop:literal, $format:ident, $default:expr) => {{
        let mut data = $default;
        match unsafe {
            $crate::mpv_get_property(
                $ctx.into(),
                cstr!($prop),
                $crate::$format,
                std::ptr::addr_of_mut!(data).cast(),
            )
        } {
            0.. => Ok(data),
            error => Err($crate::Error(error.into())),
        }
    }};
}

macro_rules! set {
    ($ctx:ident, $prop:literal, $data:expr) => {
        set!($ctx, $prop, MPV_FORMAT_STRING, cstr!($data))
    };
    ($ctx:ident, $prop:literal, bool, $data:expr) => {
        set!($ctx, $prop, MPV_FORMAT_FLAG, $data as std::ffi::c_int)
    };
    ($ctx:ident, $prop:literal, i64, $data:expr) => {
        set!($ctx, $prop, MPV_FORMAT_INT64, $data as i64)
    };
    ($ctx:ident, $prop:literal, f64, $data:expr) => {
        set!($ctx, $prop, MPV_FORMAT_DOUBLE, $data as f64)
    };
    ($ctx:ident, $prop:literal, $format:ident, $data:expr) => {{
        let data = $data;
        match unsafe {
            $crate::mpv_set_property(
                $ctx.into(),
                cstr!($prop),
                $crate::$format,
                std::ptr::addr_of!(data).cast_mut().cast(),
            )
        } {
            0.. => Ok(()),
            error => Err($crate::Error(error.into())),
        }
    }};
}

macro_rules! observe {
    ($ctx:ident, $($prop:literal),+ $(,)?) => {
        $(observe!($ctx, 0, $prop, MPV_FORMAT_NONE));+
    };
    ($ctx:ident, $format:ident, $($prop:literal),+ $(,)?) => {
        $(observe!($ctx, 0, $prop, $format));+
    };
    ($ctx:ident, $userdata:expr, $prop:literal, $format:ident) => {{
        let userdata = $userdata;
        unsafe {
            $crate::mpv_observe_property(
                $ctx.into(),
                userdata,
                cstr!($prop),
                $crate::$format,
            );
        }
    }};
}

macro_rules! unobserve {
    ($ctx:ident$(, $userdata:ident)+)=> {
        $(unsafe { mpv_unobserve_property($ctx.into(), $userdata); })+
    };
}
