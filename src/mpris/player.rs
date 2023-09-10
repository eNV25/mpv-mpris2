use std::{collections::HashMap, io, path::Path, result, time::Duration};

use data_encoding::BASE64;
use smol::{future::FutureExt, process::Command, Timer};
use url::Url;
use zbus::{dbus_interface, zvariant};

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
        _ = command!(self.ctx(), "playlist-next\0");
    }

    /// CanGoPrevious property
    #[dbus_interface(property)]
    fn can_go_previous(&self) -> bool {
        true
    }

    /// Previous method
    fn previous(&self) {
        _ = command!(self.ctx(), "playlist-prev\0");
    }

    /// CanPause property
    #[dbus_interface(property)]
    fn can_pause(&self) -> bool {
        true
    }

    /// Pause method
    fn pause(&self) {
        _ = set_bool!(self.ctx(), "pause\0", true);
    }

    /// PlayPause method
    fn play_pause(&self) {
        _ = command!(self.ctx(), "cycle\0", "pause\0");
    }

    /// CanPlay property
    #[dbus_interface(property)]
    fn can_play(&self) -> bool {
        true
    }

    /// Play method
    fn play(&self) {
        _ = set_bool!(self.ctx(), "pause\0", false);
    }

    /// CanSeek property
    #[dbus_interface(property)]
    fn can_seek(&self) -> Result<bool> {
        get_bool!(self.ctx(), "seekable\0").map_err(From::from)
    }

    /// Seek method
    fn seek(&self, offset: i64) {
        _ = command!(self.ctx(), "seek\0", format!("{}\0", (offset as f64) / 1E6));
    }

    /// Seeked signal
    #[dbus_interface(signal)]
    pub async fn seeked(ctxt: &zbus::SignalContext<'_>, position: i64) -> zbus::Result<()>;

    // OpenUri method
    fn open_uri(&self, uri: &str) {
        _ = command!(self.ctx(), "loadfile\0", format!("{}\0", uri));
    }

    /// CanControl property
    #[dbus_interface(property)]
    fn can_control(&self) -> bool {
        true
    }

    /// Stop method
    fn stop(&self) {
        _ = command!(self.ctx(), "stop\0");
    }

    /// PlaybackStatus property
    #[dbus_interface(property)]
    fn playback_status(&self) -> Result<&str> {
        if get_bool!(self.ctx(), "idle-active\0")? || get_bool!(self.ctx(), "eof-reached\0")? {
            Ok("Stopped")
        } else if get_bool!(self.ctx(), "pause\0")? {
            Ok("Paused")
        } else {
            Ok("Playing")
        }
    }

    /// LoopStatus property
    #[dbus_interface(property)]
    fn loop_status(&self) -> Result<&str> {
        let err = || Error::Failed("cannot get property".into());
        if get!(self.ctx(), "loop-file\0").ok_or_else(err)? != "no" {
            Ok("Track")
        } else if get!(self.ctx(), "loop-playlist\0").ok_or_else(err)? != "no" {
            Ok("Playlist")
        } else {
            Ok("None")
        }
    }

    #[dbus_interface(property)]
    fn set_loop_status(&self, value: &str) {
        _ = set!(
            self.ctx(),
            "loop-file\0",
            match value {
                "Track" => "inf\0",
                _ => "no\0",
            }
        );
        _ = set!(
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
    fn rate(&self) -> Result<f64> {
        get_float!(self.ctx(), "speed\0").map_err(From::from)
    }

    #[dbus_interface(property)]
    fn set_rate(&self, value: f64) {
        _ = set_float!(self.ctx(), "speed\0", value);
    }

    /// MinimumRate property
    #[dbus_interface(property)]
    fn minimum_rate(&self) -> Result<f64> {
        get_float!(self.ctx(), "option-info/speed/min\0").map_err(From::from)
    }

    /// MaximumRate property
    #[dbus_interface(property)]
    fn maximum_rate(&self) -> Result<f64> {
        get_float!(self.ctx(), "option-info/speed/max\0").map_err(From::from)
    }

    /// Shuffle property
    #[dbus_interface(property)]
    fn shuffle(&self) -> Result<bool> {
        get_bool!(self.ctx(), "shuffle\0").map_err(From::from)
    }

    #[dbus_interface(property)]
    fn set_shuffle(&self, value: bool) {
        _ = set_bool!(self.ctx(), "shuffle\0", value);
    }

    /// Metadata property
    #[dbus_interface(property)]
    async fn metadata(&self) -> Result<HashMap<&str, zvariant::Value>> {
        let (path, stream) = (
            get!(self.ctx(), "path\0").unwrap_or_default(),
            get!(self.ctx(), "stream-open-filename\0").unwrap_or_default(),
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
                        Timer::after(Duration::from_secs(1)).await;
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
                                Timer::after(Duration::from_secs(5)).await;
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

        let mut m = HashMap::new();

        if let Some(s) = get!(self.ctx(), "media-title\0") {
            m.insert("xesam:title", s.to_owned().into());
        }

        if let Some(data) = get!(self.ctx(), "metadata\0") {
            let data: HashMap<&str, String> =
                serde_json::from_str(data.into()).map_err(|err| Error::Failed(err.to_string()))?;
            for (key, value) in data {
                let integer = || -> i64 {
                    value
                        .find('/')
                        .map_or_else(|| &value[..], |x| &value[..x])
                        .parse()
                        .unwrap_or_default()
                };
                let (key, value) = match key.to_ascii_lowercase().as_str() {
                    "album" => ("xesam:album", value.into()),
                    "title" => ("xesam:title", value.into()),
                    "album_artist" => ("xesam:albumArtist", vec![value].into()),
                    "artist" => ("xesam:artist", vec![value].into()),
                    "comment" => ("xesam:comment", vec![value].into()),
                    "composer" => ("xesam:composer", vec![value].into()),
                    "genre" => ("xesam:genre", vec![value].into()),
                    "lyricist" => ("xesam:lyricist", vec![value].into()),
                    "tbp" | "tbpm" | "bpm" => ("xesam:audioBPM", integer().into()),
                    "disc" => ("xesam:discNumber", integer().into()),
                    "track" => ("xesam:trackNumber", integer().into()),
                    lyrics if lyrics.strip_prefix("lyrics").is_some() => {
                        ("xesam:asText", value.into())
                    }
                    _ => continue,
                };
                m.insert(key, value);
            }
        }

        m.insert(
            "mpris:trackid",
            zvariant::ObjectPath::try_from("/io/mpv")
                .map_err(|err| Error::ZBus(err.into()))?
                .into(),
        );

        m.insert(
            "mpris:length",
            ((get_float!(self.ctx(), "duration\0")? * 1E6) as i64).into(),
        );

        if let Some(url) = Url::parse(path).ok().or_else(|| {
            get!(self.ctx(), "working-directory\0")
                .and_then(|dir| Url::from_file_path(Path::new(dir.into()).join(path)).ok())
        }) {
            m.insert("mpris:url", url.as_str().to_owned().into());
        }

        if let Some(url) = thumb.await {
            m.insert("mpris:artUrl", url.into());
        }

        Ok(m)
    }

    /// Volume property
    #[dbus_interface(property)]
    fn volume(&self) -> Result<f64> {
        Ok(get_float!(self.ctx(), "volume\0")? / 100.0)
    }

    #[dbus_interface(property)]
    fn set_volume(&self, value: f64) {
        _ = set_float!(self.ctx(), "volume\0", value * 100.0);
    }

    /// Position property
    #[dbus_interface(property)]
    fn position(&self) -> Result<i64> {
        Ok((get_float!(self.ctx(), "playback-time\0")? * 1E6) as i64)
    }

    // SetPosition method
    fn set_position(&self, track_id: zvariant::ObjectPath<'_>, position: i64) {
        _ = track_id;
        _ = set_float!(self.ctx(), "playback-time\0", (position as f64) / 1E6);
    }
}

type Error = zbus::fdo::Error;
type Result<T = (), E = Error> = result::Result<T, E>;
