#![macro_use]

macro_rules! assert_cstr {
    ($s:expr) => {{
        debug_assert_eq!($s.as_bytes().last(), Some(&b'\0'));
        $s
    }};
}

macro_rules! command {
    ($ctx:expr, $($arg:expr),+ $(,)?) => {{
        let (ctx, args) = ($ctx, [$(assert_cstr!($arg).as_ptr()),+, std::ptr::null()]);
        match unsafe { $crate::mpv_command(ctx, std::ptr::addr_of!(args).cast_mut().cast()) } {
            0.. => Ok(()),
            error => Err($crate::Error(error.into())),
        }
    }};
}

macro_rules! get_format {
    ($ctx:expr, $prop:expr, $format:expr, $type:ty) => {{
        let (ctx, prop, format, mut data) =
            ($ctx, assert_cstr!($prop), $format, <$type>::default());
        match unsafe {
            $crate::mpv_get_property(
                ctx,
                prop.as_ptr().cast(),
                format,
                std::ptr::addr_of_mut!(data).cast(),
            )
        } {
            0.. => Ok(data),
            error => Err($crate::Error(error.into())),
        }
    }};
}

macro_rules! get_bool {
    ($ctx:expr, $prop:expr) => {
        get_format!($ctx, $prop, $crate::MPV_FORMAT_FLAG, std::ffi::c_int).map(|x| x != 0)
    };
}

macro_rules! get_float {
    ($ctx:expr, $prop:expr) => {
        get_format!($ctx, $prop, $crate::MPV_FORMAT_DOUBLE, f64)
    };
}

macro_rules! get {
    ($ctx:expr, $prop:expr) => {{
        let (ctx, prop) = ($ctx, $prop);
        unsafe { $crate::mpv_get_property_string(ctx, prop.as_ptr().cast()).as_ref() }
            .and_then(|s| $crate::Str::try_from(s).ok())
    }};
}

macro_rules! set_format {
    ($ctx:expr, $prop:expr, $format:expr, $data:expr) => {{
        let (ctx, prop, format, data) = ($ctx, assert_cstr!($prop), $format, $data);
        match unsafe {
            $crate::mpv_set_property(
                ctx,
                prop.as_ptr().cast(),
                format,
                std::ptr::addr_of!(data).cast_mut().cast(),
            )
        } {
            0.. => Ok(()),
            error => Err($crate::Error(error.into())),
        }
    }};
}

macro_rules! set_bool {
    ($ctx:expr, $prop:expr, $value:expr) => {
        set_format!(
            $ctx,
            $prop,
            $crate::MPV_FORMAT_FLAG,
            $value as std::ffi::c_int
        )
    };
}

macro_rules! set_float {
    ($ctx:expr, $prop:expr, $data:expr) => {
        set_format!($ctx, $prop, $crate::MPV_FORMAT_DOUBLE, $data as f64)
    };
}

macro_rules! set {
    ($ctx:expr, $prop:expr, $data:expr) => {
        set_format!(
            $ctx,
            assert_cstr!($prop),
            $crate::MPV_FORMAT_STRING,
            assert_cstr!($data).as_ptr()
        )
    };
}

macro_rules! observe_format {
    ($ctx:expr, $userdata:expr, $prop:expr, $format:expr) => {{
        let (ctx, userdata, prop, format) = ($ctx, $userdata, assert_cstr!($prop), $format);
        unsafe {
            $crate::mpv_observe_property(ctx, userdata, prop.as_ptr().cast(), format);
        }
    }};
}

macro_rules! observe {
    ($ctx:expr, $($prop:expr),+ $(,)?) => {
        $(observe_format!($ctx, $crate::MPV_MPRIS, $prop, $crate::MPV_FORMAT_NONE));+
    };
}

macro_rules! unobserve {
    ($ctx:expr$(, $userdata:expr)+)=> {
        $({
            let (ctx, userdata) = ($ctx, $userdata);
            unsafe { mpv_unobserve_property(ctx, userdata); }
        })+
    };
}
