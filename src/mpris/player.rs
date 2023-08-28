use std::{collections, io, path, time};

use data_encoding::BASE64;
use smol::{future::FutureExt, process::Command};
use url::Url;
use zbus::dbus_interface;

#[repr(transparent)]
pub struct PlayerImpl {
    ctx: crate::Handle,
}

impl From<*mut crate::mpv_handle> for PlayerImpl {
    fn from(value: *mut crate::mpv_handle) -> Self {
        Self {
            ctx: crate::Handle(value),
        }
    }
}

impl PlayerImpl {
    #[inline]
    fn ctx(&self) -> *mut crate::mpv_handle {
        self.ctx.0
    }
}

#[dbus_interface(name = "org.mpris.MediaPlayer2.Player")]
impl PlayerImpl {
    /// CanGoNext property
    #[dbus_interface(property)]
    fn can_go_next(&self) -> bool {
        true
    }

    /// Next method
    fn next(&self) {
        command!(self.ctx(), "playlist-next\0");
    }

    /// CanGoPrevious property
    #[dbus_interface(property)]
    fn can_go_previous(&self) -> bool {
        true
    }

    /// Previous method
    fn previous(&self) {
        command!(self.ctx(), "playlist-prev\0");
    }

    /// CanPause property
    #[dbus_interface(property)]
    fn can_pause(&self) -> bool {
        true
    }

    /// Pause method
    fn pause(&self) {
        set_property_bool!(self.ctx(), "pause\0", true);
    }

    /// PlayPause method
    fn play_pause(&self) {
        command!(self.ctx(), "cycle\0", "pause\0");
    }

    /// CanPlay property
    #[dbus_interface(property)]
    fn can_play(&self) -> bool {
        true
    }

    /// Play method
    fn play(&self) {
        set_property_bool!(self.ctx(), "pause\0", false);
    }

    /// CanSeek property
    #[dbus_interface(property)]
    fn can_seek(&self) -> bool {
        get_property_bool!(self.ctx(), "seekable\0")
    }

    /// Seek method
    fn seek(&self, offset: i64) {
        command!(self.ctx(), "seek\0", format!("{}\0", (offset as f64) / 1E6));
    }

    /// Seeked signal
    #[dbus_interface(signal)]
    pub async fn seeked(ctxt: &zbus::SignalContext<'_>, position: i64) -> zbus::Result<()>;

    // OpenUri method
    fn open_uri(&self, uri: &str) {
        command!(self.ctx(), "loadfile\0", format!("{}\0", uri));
    }

    /// CanControl property
    #[dbus_interface(property)]
    fn can_control(&self) -> bool {
        true
    }

    /// Stop method
    fn stop(&self) {
        command!(self.ctx(), "stop\0");
    }

    /// PlaybackStatus property
    #[dbus_interface(property)]
    fn playback_status(&self) -> &str {
        if get_property_bool!(self.ctx(), "idle-active\0")
            || get_property_bool!(self.ctx(), "eof-reached\0")
        {
            "Stopped"
        } else if get_property_bool!(self.ctx(), "pause\0") {
            "Paused"
        } else {
            "Playing"
        }
    }

    /// LoopStatus property
    #[dbus_interface(property)]
    fn loop_status(&self) -> &str {
        if matches!(get_property!(self.ctx(), "loop-file\0"), Some(v) if v != "no") {
            "Track"
        } else if matches!(get_property!(self.ctx(), "loop-playlist\0"), Some(v) if v != "no") {
            "Playlist"
        } else {
            "None"
        }
    }

    #[dbus_interface(property)]
    fn set_loop_status(&self, value: &str) {
        set_property!(
            self.ctx(),
            "loop-file\0",
            match value {
                "Track" => "inf\0",
                _ => "no\0",
            }
        );
        set_property!(
            self.ctx(),
            "loop-playlist\0",
            match value {
                "Playlist" => "inf\0",
                _ => "no\0",
            }
        );
    }

    /// Rate property
    #[dbus_interface(property)]
    fn rate(&self) -> f64 {
        get_property_float!(self.ctx(), "speed\0")
    }

    #[dbus_interface(property)]
    fn set_rate(&self, value: f64) {
        set_property_float!(self.ctx(), "speed\0", value);
    }

    /// MinimumRate property
    #[dbus_interface(property)]
    fn minimum_rate(&self) -> f64 {
        get_property_float!(self.ctx(), "option-info/speed/min\0")
    }

    /// MaximumRate property
    #[dbus_interface(property)]
    fn maximum_rate(&self) -> f64 {
        get_property_float!(self.ctx(), "option-info/speed/max\0")
    }

    /// Shuffle property
    #[dbus_interface(property)]
    fn shuffle(&self) -> bool {
        get_property_bool!(self.ctx(), "shuffle\0")
    }

    #[dbus_interface(property)]
    fn set_shuffle(&self, value: bool) {
        set_property_bool!(self.ctx(), "shuffle\0", value);
    }

    /// Metadata property
    #[dbus_interface(property)]
    async fn metadata(&self) -> collections::HashMap<&str, zbus::zvariant::Value> {
        let (path, stream) = (
            get_property!(self.ctx(), "path\0").unwrap_or_default(),
            get_property!(self.ctx(), "stream-open-filename\0").unwrap_or_default(),
        );
        let (path, stream) = (path.into_str(), stream.into_str());

        let thumb = smol::spawn(async {
            let (path, stream) = (path.to_owned(), stream.to_owned());
            if path == stream {
                Command::new("ffmpegthumbnailer")
                    .args(["-m", "-cjpeg", "-s0", "-o-", "-i"])
                    .arg(&path)
                    .output()
                    .or(async {
                        smol::Timer::after(time::Duration::from_secs(1)).await;
                        Err(io::ErrorKind::TimedOut.into())
                    })
                    .await
                    .ok()
                    .map(|output| BASE64.encode(&output.stdout))
                    .map(|data| format!("data:image/jpeg;base64,{data}"))
            } else {
                'ytdl: {
                    for cmd in ["yt-dlp", "yt-dlp_x86", "youtube-dl"] {
                        let thumb = Command::new(cmd)
                            .args(["--no-warnings", "--get-thumbnail"])
                            .arg(&path)
                            .output()
                            .or(async {
                                smol::Timer::after(time::Duration::from_secs(5)).await;
                                Err(io::ErrorKind::TimedOut.into())
                            })
                            .await
                            .ok()
                            .and_then(|output| {
                                std::str::from_utf8(&output.stdout)
                                    .map(|s| s.trim().to_owned())
                                    .ok()
                            });
                        if thumb.is_some() {
                            break 'ytdl thumb;
                        }
                    }
                    None
                }
            }
        });

        let mut m = collections::HashMap::new();

        if let Some(s) = get_property!(self.ctx(), "media-title\0") {
            m.insert("xesam:title", s.to_owned().into());
        }

        if let Some(data) = get_property!(self.ctx(), "metadata\0") {
            let data: collections::HashMap<&str, String> =
                serde_json::from_str(data.into()).unwrap_or_default();
            for (key, value) in data {
                let integer = || -> i64 {
                    value
                        .find('/')
                        .map_or_else(|| &value[..], |x| &value[..x])
                        .parse()
                        .unwrap_or_default()
                };
                match key.to_ascii_lowercase().as_str() {
                    "album" => m.insert("xesam:album", value.into()),
                    "title" => m.insert("xesam:title", value.into()),
                    "album_artist" => m.insert("xesam:albumArtist", vec![value].into()),
                    "artist" => m.insert("xesam:artist", vec![value].into()),
                    "comment" => m.insert("xesam:comment", vec![value].into()),
                    "composer" => m.insert("xesam:composer", vec![value].into()),
                    "genre" => m.insert("xesam:genre", vec![value].into()),
                    "lyricist" => m.insert("xesam:lyricist", vec![value].into()),
                    "tbp" | "tbpm" | "bpm" => m.insert("xesam:audioBPM", integer().into()),
                    "disc" => m.insert("xesam:discNumber", integer().into()),
                    "track" => m.insert("xesam:trackNumber", integer().into()),
                    lyrics if lyrics.strip_prefix("lyrics").is_some() => {
                        m.insert("xesam:asText", value.into())
                    }
                    _ => None,
                };
            }
        }

        if let Ok(path) = zbus::zvariant::ObjectPath::try_from("/io/mpv") {
            m.insert("mpris:trackid", path.into());
        }

        m.insert(
            "mpris:length",
            ((get_property_float!(self.ctx(), "duration\0") * 1E6) as i64).into(),
        );

        if let Some(url) = Url::parse(path).ok().or_else(|| {
            get_property!(self.ctx(), "working-directory\0")
                .and_then(|dir| Url::from_file_path(path::Path::new(dir.into()).join(path)).ok())
        }) {
            m.insert("mpris:url", url.as_str().to_owned().into());
        }

        if let Some(url) = thumb.await {
            m.insert("mpris:artUrl", url.into());
        }

        m
    }

    /// Volume property
    #[dbus_interface(property)]
    fn volume(&self) -> f64 {
        get_property_float!(self.ctx(), "volume\0") / 100.0
    }

    #[dbus_interface(property)]
    fn set_volume(&self, value: f64) {
        set_property_float!(self.ctx(), "volume\0", value * 100.0);
    }

    /// Position property
    #[dbus_interface(property)]
    fn position(&self) -> i64 {
        (get_property_float!(self.ctx(), "playback-time\0") * 1E6) as i64
    }

    // SetPosition method
    fn set_position(&self, track_id: zbus::zvariant::ObjectPath<'_>, position: i64) {
        _ = track_id;
        set_property_float!(self.ctx(), "playback-time\0", (position as f64) / 1E6);
    }
}
