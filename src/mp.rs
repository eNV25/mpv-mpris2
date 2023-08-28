macro_rules! assert_cstr {
    ($s:expr) => {{
        let s = $s;
        debug_assert_eq!(s.as_bytes()[s.len() - 1], '\0' as u8);
        $s
    }};
}

macro_rules! command {
    ($ctx:expr, $($arg:expr),+ $(,)?) => {{
        let (ctx, args) = ($ctx, [$(assert_cstr!($arg).as_ptr()),+, std::ptr::null()]);
        unsafe {
            $crate::mpv_command_async(ctx, $crate::REPLY_USERDATA, (&args).as_ptr().cast_mut().cast());
        }
    }};
}

macro_rules! get_property_format {
    ($ctx:expr, $prop:expr, $format:expr, $type:ty) => {{
        let (ctx, prop, format, mut data) =
            ($ctx, assert_cstr!($prop), $format, [<$type>::default()]);
        unsafe {
            $crate::mpv_get_property(
                ctx,
                prop.as_ptr().cast(),
                format,
                (&mut data).as_mut_ptr().cast(),
            );
        }
        data[0]
    }};
}

macro_rules! get_property_bool {
    ($ctx:expr, $prop:expr) => {
        get_property_format!($ctx, $prop, $crate::MPV_FORMAT_FLAG, std::ffi::c_int) != 0
    };
}

macro_rules! get_property_float {
    ($ctx:expr, $prop:expr) => {
        get_property_format!($ctx, $prop, $crate::MPV_FORMAT_DOUBLE, f64)
    };
}

macro_rules! get_property {
    ($ctx:expr, $prop:expr) => {{
        let (ctx, prop) = ($ctx, $prop);
        unsafe { $crate::mpv_get_property_string(ctx, prop.as_ptr().cast()).as_ref() }
            .and_then(|s| $crate::Str::try_from(s).ok())
    }};
}

macro_rules! set_property_format {
    ($ctx:expr, $prop:expr, $format:expr, $data:expr) => {{
        let (ctx, prop, format, data) = ($ctx, assert_cstr!($prop), $format, [$data]);
        unsafe {
            $crate::mpv_set_property_async(
                ctx,
                $crate::REPLY_USERDATA,
                prop.as_ptr().cast(),
                format,
                (&data).as_ptr().cast_mut().cast(),
            );
        }
    }};
}

macro_rules! set_property_bool {
    ($ctx:expr, $prop:expr, $value:expr) => {
        set_property_format!(
            $ctx,
            $prop,
            $crate::MPV_FORMAT_FLAG,
            $value as std::ffi::c_int
        )
    };
}

macro_rules! set_property_float {
    ($ctx:expr, $prop:expr, $data:expr) => {
        set_property_format!($ctx, $prop, $crate::MPV_FORMAT_DOUBLE, $data as f64)
    };
}

macro_rules! set_property {
    ($ctx:expr, $prop:expr, $data:expr) => {
        set_property_format!(
            $ctx,
            assert_cstr!($prop),
            $crate::MPV_FORMAT_STRING,
            assert_cstr!($data).as_ptr()
        )
    };
}

macro_rules! observe_property_format {
    ($ctx:expr, $prop:expr, $format:expr) => {{
        let (ctx, prop, format) = ($ctx, assert_cstr!($prop), $format);
        unsafe {
            $crate::mpv_observe_property(ctx, $crate::REPLY_USERDATA, prop.as_ptr().cast(), format);
        }
    }};
}

macro_rules! observe_properties {
    ($ctx:expr, $($prop:expr),+ $(,)?) => {
        $(observe_property_format!($ctx, $prop, $crate::MPV_FORMAT_STRING));+
    };
}
