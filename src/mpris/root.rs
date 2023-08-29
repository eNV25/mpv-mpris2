use std::{
    env, fs,
    io::{self, BufRead},
    path, result,
};

use zbus::dbus_interface;

#[repr(transparent)]
pub struct RootImpl {
    ctx: crate::Handle,
}

impl From<*mut crate::mpv_handle> for RootImpl {
    fn from(value: *mut crate::mpv_handle) -> Self {
        Self {
            ctx: crate::Handle(value),
        }
    }
}

impl RootImpl {
    #[inline]
    fn ctx(&self) -> *mut crate::mpv_handle {
        self.ctx.0
    }
}

#[dbus_interface(name = "org.mpris.MediaPlayer2")]
impl RootImpl {
    /// DesktopEntry property
    #[dbus_interface(property)]
    fn desktop_entry(&self) -> &str {
        "mpv"
    }

    /// Identity property
    #[dbus_interface(property)]
    fn identity(&self) -> &str {
        "mpv Media Player"
    }

    /// SupportedMimeTypes property
    #[dbus_interface(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        env::var("XDG_DATA_DIRS")
            .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_owned())
            .split(':')
            .map(path::Path::new)
            .filter(|&path| path.is_absolute())
            .map(|dir| dir.join("applications/mpv.desktop"))
            .filter_map(|path| fs::File::open(path).ok())
            .flat_map(|f| io::BufReader::new(f).lines())
            .filter_map(result::Result::ok)
            .find_map(|line| line.strip_prefix("MimeType=").map(str::to_owned))
            .map_or_else(Vec::new, |v| {
                v.split_terminator(';').map(str::to_owned).collect()
            })
    }

    /// SupportedUriSchemes property
    #[dbus_interface(property)]
    fn supported_uri_schemes(&self) -> Result<Vec<String>> {
        get_property!(self.ctx(), "protocol-list\0")
            .ok_or_else(|| Error::Failed("cannot get property".into()))
            .map(|x| x.split(',').map(str::to_owned).collect())
    }

    /// CanQuit property
    #[dbus_interface(property)]
    fn can_quit(&self) -> bool {
        true
    }

    /// Quit method
    fn quit(&self) {
        _ = command!(self.ctx(), "quit\0");
    }

    /// CanRaise property
    #[dbus_interface(property)]
    fn can_raise(&self) -> bool {
        false
    }

    /// CanSetFullscreen property
    #[dbus_interface(property)]
    fn can_set_fullscreen(&self) -> bool {
        true
    }

    /// Fullscreen property
    #[dbus_interface(property)]
    fn fullscreen(&self) -> Result<bool> {
        get_property_bool!(self.ctx(), "fullscreen\0").map_err(From::from)
    }

    /// Fullscreen property setter
    #[dbus_interface(property)]
    fn set_fullscreen(&self, value: bool) {
        _ = set_property_bool!(self.ctx(), "fullscreen\0", value);
    }

    /// HasTrackList property
    #[dbus_interface(property)]
    fn has_track_list(&self) -> bool {
        false
    }
}

type Error = zbus::fdo::Error;
type Result<T = ()> = zbus::fdo::Result<T>;
