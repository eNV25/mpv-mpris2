use std::sync;

use zbus::dbus_interface;

use crate::mpv;

#[repr(transparent)]
pub struct RootProxy {
    ctx: mpv::Handle,
}

impl From<*mut mpv::capi::mpv_handle> for RootProxy {
    fn from(value: *mut mpv::capi::mpv_handle) -> Self {
        Self {
            ctx: mpv::Handle(value),
        }
    }
}

impl RootProxy {
    #[inline(always)]
    fn ctx(&self) -> *mut mpv::capi::mpv_handle {
        self.ctx.0
    }
}

#[dbus_interface(name = "org.mpris.MediaPlayer2")]
impl RootProxy {
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
    fn supported_mime_types(&self) -> &[&str] {
        //static MIME_TYPES: sync::OnceLock<Vec<String>> = sync::OnceLock::new();
        //MIME_TYPES.get_or_init(|| {
        //    env::var("XDG_DATA_DIRS")
        //        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_owned())
        //        .split(":")
        //        .map(path::Path::new)
        //        .filter(|&path| path.is_absolute())
        //        .map(|dir| dir.join("applications/mpv.desktop"))
        //        .filter_map(|path| fs::File::open(path).ok())
        //        .flat_map(|f| io::BufReader::new(f).lines())
        //        .filter_map(Result::ok)
        //        .find_map(|line| line.strip_prefix("MimeType=").map(str::to_owned))
        //        .map_or_else(
        //            || vec![],
        //            |v| v.split_terminator(";").map(str::to_owned).collect(),
        //        )
        //})
        &[
            "application/ogg",
            "application/x-ogg",
            "application/mxf",
            "application/sdp",
            "application/smil",
            "application/x-smil",
            "application/streamingmedia",
            "application/x-streamingmedia",
            "application/vnd.rn-realmedia",
            "application/vnd.rn-realmedia-vbr",
            "audio/aac",
            "audio/x-aac",
            "audio/vnd.dolby.heaac.1",
            "audio/vnd.dolby.heaac.2",
            "audio/aiff",
            "audio/x-aiff",
            "audio/m4a",
            "audio/x-m4a",
            "application/x-extension-m4a",
            "audio/mp1",
            "audio/x-mp1",
            "audio/mp2",
            "audio/x-mp2",
            "audio/mp3",
            "audio/x-mp3",
            "audio/mpeg",
            "audio/mpeg2",
            "audio/mpeg3",
            "audio/mpegurl",
            "audio/x-mpegurl",
            "audio/mpg",
            "audio/x-mpg",
            "audio/rn-mpeg",
            "audio/musepack",
            "audio/x-musepack",
            "audio/ogg",
            "audio/scpls",
            "audio/x-scpls",
            "audio/vnd.rn-realaudio",
            "audio/wav",
            "audio/x-pn-wav",
            "audio/x-pn-windows-pcm",
            "audio/x-realaudio",
            "audio/x-pn-realaudio",
            "audio/x-ms-wma",
            "audio/x-pls",
            "audio/x-wav",
            "video/mpeg",
            "video/x-mpeg2",
            "video/x-mpeg3",
            "video/mp4v-es",
            "video/x-m4v",
            "video/mp4",
            "application/x-extension-mp4",
            "video/divx",
            "video/vnd.divx",
            "video/msvideo",
            "video/x-msvideo",
            "video/ogg",
            "video/quicktime",
            "video/vnd.rn-realvideo",
            "video/x-ms-afs",
            "video/x-ms-asf",
            "audio/x-ms-asf",
            "application/vnd.ms-asf",
            "video/x-ms-wmv",
            "video/x-ms-wmx",
            "video/x-ms-wvxvideo",
            "video/x-avi",
            "video/avi",
            "video/x-flic",
            "video/fli",
            "video/x-flc",
            "video/flv",
            "video/x-flv",
            "video/x-theora",
            "video/x-theora+ogg",
            "video/x-matroska",
            "video/mkv",
            "audio/x-matroska",
            "application/x-matroska",
            "video/webm",
            "audio/webm",
            "audio/vorbis",
            "audio/x-vorbis",
            "audio/x-vorbis+ogg",
            "video/x-ogm",
            "video/x-ogm+ogg",
            "application/x-ogm",
            "application/x-ogm-audio",
            "application/x-ogm-video",
            "application/x-shorten",
            "audio/x-shorten",
            "audio/x-ape",
            "audio/x-wavpack",
            "audio/x-tta",
            "audio/AMR",
            "audio/ac3",
            "audio/eac3",
            "audio/amr-wb",
            "video/mp2t",
            "audio/flac",
            "audio/mp4",
            "application/x-mpegurl",
            "video/vnd.mpegurl",
            "application/vnd.apple.mpegurl",
            "audio/x-pn-au",
            "video/3gp",
            "video/3gpp",
            "video/3gpp2",
            "audio/3gpp",
            "audio/3gpp2",
            "video/dv",
            "audio/dv",
            "audio/opus",
            "audio/vnd.dts",
            "audio/vnd.dts.hd",
            "audio/x-adpcm",
            "application/x-cue",
            "audio/m3u",
        ]
    }

    /// SupportedUriSchemes property
    #[dbus_interface(property)]
    fn supported_uri_schemes(&self) -> &'static [String] {
        static URI_SCHEMES: sync::OnceLock<Vec<String>> = sync::OnceLock::new();
        URI_SCHEMES.get_or_init(|| {
            mpv::get_property_string!(self.ctx(), "protocol-list\0")
                .split(',')
                .map(str::to_owned)
                .collect()
        })
    }

    /// CanQuit property
    #[dbus_interface(property)]
    fn can_quit(&self) -> bool {
        true
    }

    /// Quit method
    fn quit(&self) {
        mpv::command!(self.ctx(), "quit\0");
    }

    /// CanRaise property
    #[dbus_interface(property)]
    fn can_raise(&self) -> bool {
        true
    }

    /// Raise method
    fn raise(&self) {
        mpv::set_property_bool!(self.ctx(), "focused\0", true);
    }

    /// CanSetFullscreen property
    #[dbus_interface(property)]
    fn can_set_fullscreen(&self) -> bool {
        true
    }

    /// Fullscreen property
    #[dbus_interface(property)]
    fn fullscreen(&self) -> bool {
        mpv::get_property_bool!(self.ctx(), "fullscreen\0")
    }

    /// Fullscreen property setter
    #[dbus_interface(property)]
    fn set_fullscreen(&self, value: bool) {
        mpv::set_property_bool!(self.ctx(), "fullscreen\0", value);
    }

    /// HasTrackList property
    #[dbus_interface(property)]
    fn has_track_list(&self) -> bool {
        false
    }
}
