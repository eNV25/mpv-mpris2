#![macro_use]

macro_rules! strc {
    ($s:literal) => {
        concat!($s, "\0").as_ptr().cast::<std::ffi::c_char>()
    };
    ($s:expr) => {{
        debug_assert_eq!($crate::AsBytes::as_bytes($s).last(), Some(&b'\0'));
        $s.as_ptr().cast::<std::ffi::c_char>()
    }};
}

macro_rules! command {
    ($mpv:ident, $($arg:expr),+ $(,)?) => {{
        let args = [$(strc!($arg)),+, std::ptr::null()];
        match unsafe { $crate::mpv_command($mpv.into(), std::ptr::addr_of!(args).cast_mut().cast()) } {
            0.. => Ok(()),
            error => Err($crate::Error(error.into())),
        }
    }};
}

macro_rules! get {
    ($mpv:ident, $prop:literal) => {
        get!($mpv, $prop, MPV_FORMAT_STRING)
    };
    ($mpv:ident, $prop:literal, bool) => {
        get!($mpv, $prop, MPV_FORMAT_FLAG).map(|x| x != 0)
    };
    ($mpv:ident, $prop:literal, i64) => {
        get!($mpv, $prop, MPV_FORMAT_INT64)
    };
    ($mpv:ident, $prop:literal, f64) => {
        get!($mpv, $prop, MPV_FORMAT_DOUBLE)
    };
    ($mpv:ident, $prop:literal, MPV_FORMAT_STRING) => {
        unsafe {
            let ptr = $crate::mpv_get_property_string($mpv.into(), strc!($prop));
            if ptr.is_null() {
                "".to_owned()
            } else {
                let prop = $crate::string_from_cstr_lossy(ptr);
                $crate::mpv_free(ptr.cast());
                prop
            }
        }
    };
    ($mpv:ident, $prop:literal, MPV_FORMAT_FLAG) => {
        get!($mpv, $prop, MPV_FORMAT_FLAG, std::ffi::c_int::default())
    };
    ($mpv:ident, $prop:literal, MPV_FORMAT_INT64) => {
        get!($mpv, $prop, MPV_FORMAT_INT64, i64::default())
    };
    ($mpv:ident, $prop:literal, MPV_FORMAT_DOUBLE) => {
        get!($mpv, $prop, MPV_FORMAT_DOUBLE, f64::default())
    };
    ($mpv:ident, $prop:literal, $format:ident, $default:expr) => {{
        let mut data = $default;
        match unsafe {
            $crate::mpv_get_property(
                $mpv.into(),
                strc!($prop),
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
    ($mpv:ident, $prop:literal, $data:expr) => {
        set!($mpv, $prop, MPV_FORMAT_STRING, strc!($data))
    };
    ($mpv:ident, $prop:literal, bool, $data:expr) => {
        set!($mpv, $prop, MPV_FORMAT_FLAG, std::ffi::c_int::from($data))
    };
    ($mpv:ident, $prop:literal, i64, $data:expr) => {
        set!($mpv, $prop, MPV_FORMAT_INT64, $data as i64)
    };
    ($mpv:ident, $prop:literal, f64, $data:expr) => {
        set!($mpv, $prop, MPV_FORMAT_DOUBLE, $data as f64)
    };
    ($mpv:ident, $prop:literal, $format:ident, $data:expr) => {{
        let data = $data;
        match unsafe {
            $crate::mpv_set_property(
                $mpv.into(),
                strc!($prop),
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
    ($mpv:ident, $($prop:literal),+ $(,)?) => {
        $(observe!($mpv, 0, $prop, MPV_FORMAT_NONE));+
    };
    ($mpv:ident, $format:ident, $($prop:literal),+ $(,)?) => {
        $(observe!($mpv, 0, $prop, $format));+
    };
    ($mpv:ident, $userdata:expr, $prop:literal, $format:ident) => {{
        let userdata = $userdata;
        unsafe {
            $crate::mpv_observe_property(
                $mpv.into(),
                userdata,
                strc!($prop),
                $crate::$format,
            );
        }
    }};
}
