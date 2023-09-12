use std::{
    collections::HashMap,
    env,
    fs::File,
    io::{self, BufRead, BufReader},
    path::Path,
    result,
    time::Duration,
};

use data_encoding::BASE64;
use smol::{future::FutureExt, process::Command, Timer};
use url::Url;
use zbus::{dbus_interface, zvariant};

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Root(crate::Handle);

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Player(crate::Handle);

impl From<*mut crate::mpv_handle> for Root {
    fn from(value: *mut crate::mpv_handle) -> Self {
        Self(crate::Handle(value))
    }
}

impl From<*mut crate::mpv_handle> for Player {
    #[inline]
    fn from(value: *mut crate::mpv_handle) -> Self {
        Self(crate::Handle(value))
    }
}

impl From<Root> for *mut crate::mpv_handle {
    #[inline]
    fn from(value: Root) -> Self {
        value.0 .0
    }
}

impl From<Player> for *mut crate::mpv_handle {
    #[inline]
    fn from(value: Player) -> Self {
        value.0 .0
    }
}

#[dbus_interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    /// DesktopEntry property
    #[dbus_interface(property)]
    fn desktop_entry(self) -> &'static str {
        _ = self;
        "mpv"
    }

    /// Identity property
    #[dbus_interface(property)]
    fn identity(self) -> &'static str {
        _ = self;
        "mpv Media Player"
    }

    /// SupportedMimeTypes property
    #[dbus_interface(property)]
    fn supported_mime_types(self) -> Vec<String> {
        _ = self;
        env::var("XDG_DATA_DIRS")
            .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_owned())
            .split(':')
            .map(Path::new)
            .filter(|&path| path.is_absolute())
            .map(|dir| dir.join("applications/mpv.desktop"))
            .filter_map(|path| File::open(path).ok().map(BufReader::new))
            .flat_map(BufRead::lines)
            .filter_map(Result::ok)
            .find_map(|line| line.strip_prefix("MimeType=").map(str::to_owned))
            .map_or_else(Vec::new, |v| {
                v.split_terminator(';').map(str::to_owned).collect()
            })
    }

    /// SupportedUriSchemes property
    #[dbus_interface(property)]
    fn supported_uri_schemes(self) -> Result<Vec<String>> {
        get!(self, "protocol-list\0")
            .ok_or_else(|| Error::Failed("cannot get protocol-list".into()))
            .map(|x| x.split(',').map(str::to_owned).collect())
    }

    /// CanQuit property
    #[dbus_interface(property)]
    fn can_quit(self) -> bool {
        _ = self;
        true
    }

    /// Quit method
    fn quit(self) {
        _ = command!(self, "quit\0");
    }

    /// CanRaise property
    #[dbus_interface(property)]
    fn can_raise(self) -> bool {
        _ = self;
        false
    }

    /// CanSetFullscreen property
    #[dbus_interface(property)]
    fn can_set_fullscreen(self) -> bool {
        _ = self;
        true
    }

    /// Fullscreen property
    #[dbus_interface(property)]
    fn fullscreen(self) -> Result<bool> {
        get!(self, "fullscreen\0", bool).map_err(From::from)
    }

    /// Fullscreen property setter
    #[dbus_interface(property)]
    fn set_fullscreen(self, value: bool) {
        _ = set!(self, "fullscreen\0", bool, value);
    }

    /// HasTrackList property
    #[dbus_interface(property)]
    fn has_track_list(self) -> bool {
        _ = self;
        false
    }
}

#[dbus_interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    /// CanGoNext property
    #[dbus_interface(property)]
    fn can_go_next(self) -> bool {
        _ = self;
        true
    }

    /// Next method
    fn next(self) {
        _ = command!(self, "playlist-next\0");
    }

    /// CanGoPrevious property
    #[dbus_interface(property)]
    fn can_go_previous(self) -> bool {
        true
    }

    /// Previous method
    fn previous(self) {
        _ = command!(self, "playlist-prev\0");
    }

    /// CanPause property
    #[dbus_interface(property)]
    fn can_pause(self) -> bool {
        true
    }

    /// Pause method
    fn pause(self) {
        _ = set!(self, "pause\0", bool, true);
    }

    /// PlayPause method
    fn play_pause(self) {
        _ = command!(self, "cycle\0", "pause\0");
    }

    /// CanPlay property
    #[dbus_interface(property)]
    fn can_play(self) -> bool {
        true
    }

    /// Play method
    fn play(self) {
        _ = set!(self, "pause\0", bool, false);
    }

    /// CanSeek property
    #[dbus_interface(property)]
    fn can_seek(self) -> Result<bool> {
        get!(self, "seekable\0", bool).map_err(From::from)
    }

    /// Seek method
    fn seek(self, offset: i64) {
        _ = command!(self, "seek\0", format!("{}\0", (offset as f64) / 1E6));
    }

    /// Seeked signal
    #[dbus_interface(signal)]
    pub async fn seeked(ctxt: &zbus::SignalContext<'_>, position: i64) -> zbus::Result<()>;

    // OpenUri method
    fn open_uri(self, uri: &str) {
        _ = command!(self, "loadfile\0", format!("{}\0", uri));
    }

    /// CanControl property
    #[dbus_interface(property)]
    fn can_control(self) -> bool {
        true
    }

    /// Stop method
    fn stop(self) {
        _ = command!(self, "stop\0");
    }

    /// PlaybackStatus property
    #[dbus_interface(property)]
    fn playback_status(self) -> Result<&'static str> {
        if get!(self, "idle-active\0", bool)? || get!(self, "eof-reached\0", bool)? {
            Ok("Stopped")
        } else if get!(self, "pause\0", bool)? {
            Ok("Paused")
        } else {
            Ok("Playing")
        }
    }

    /// LoopStatus property
    #[dbus_interface(property)]
    fn loop_status(self) -> Result<&'static str> {
        let err = || Error::Failed("cannot get property".into());
        if get!(self, "loop-file\0").ok_or_else(err)? != "no" {
            Ok("Track")
        } else if get!(self, "loop-playlist\0").ok_or_else(err)? != "no" {
            Ok("Playlist")
        } else {
            Ok("None")
        }
    }

    #[dbus_interface(property)]
    fn set_loop_status(self, value: &str) {
        _ = set!(
            self,
            "loop-file\0",
            match value {
                "Track" => "inf\0",
                _ => "no\0",
            }
        );
        _ = set!(
            self,
            "loop-playlist\0",
            match value {
                "Playlist" => "inf\0",
                _ => "no\0",
            }
        );
    }

    /// Rate property
    #[dbus_interface(property)]
    fn rate(self) -> Result<f64> {
        get!(self, "speed\0", f64).map_err(From::from)
    }

    #[dbus_interface(property)]
    fn set_rate(self, value: f64) {
        _ = set!(self, "speed\0", f64, value);
    }

    /// MinimumRate property
    #[dbus_interface(property)]
    fn minimum_rate(self) -> Result<f64> {
        get!(self, "option-info/speed/min\0", f64).map_err(From::from)
    }

    /// MaximumRate property
    #[dbus_interface(property)]
    fn maximum_rate(self) -> Result<f64> {
        get!(self, "option-info/speed/max\0", f64).map_err(From::from)
    }

    /// Shuffle property
    #[dbus_interface(property)]
    fn shuffle(self) -> Result<bool> {
        get!(self, "shuffle\0", bool).map_err(From::from)
    }

    #[dbus_interface(property)]
    fn set_shuffle(self, value: bool) {
        _ = set!(self, "shuffle\0", bool, value);
    }

    /// Metadata property
    #[dbus_interface(property)]
    async fn metadata(self) -> Result<HashMap<&'static str, zvariant::OwnedValue>> {
        macro_rules! value {
            ($value:expr) => {
                zvariant::Value::from($value).to_owned()
            };
        }

        let thumb = smol::spawn(async move {
            let path = get!(self, "path\0").unwrap_or_default();
            if path == get!(self, "stream-open-filename\0").unwrap_or_default() {
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

        if let Some(s) = get!(self, "media-title\0") {
            m.insert("xesam:title", value!(s));
        }

        if let Some(data) = get!(self, "metadata\0") {
            let data: HashMap<&str, String> =
                serde_json::from_str(&data).map_err(|err| Error::Failed(err.to_string()))?;
            for (key, value) in data {
                let integer = || -> i64 {
                    value
                        .find('/')
                        .map_or_else(|| &value[..], |x| &value[..x])
                        .parse()
                        .unwrap_or_default()
                };
                let (key, value) = match key.to_ascii_lowercase().as_str() {
                    "album" => ("xesam:album", value!(value)),
                    "title" => ("xesam:title", value!(value)),
                    "album_artist" => ("xesam:albumArtist", value!(vec![value])),
                    "artist" => ("xesam:artist", value!(vec![value])),
                    "comment" => ("xesam:comment", value!(vec![value])),
                    "composer" => ("xesam:composer", value!(vec![value])),
                    "genre" => ("xesam:genre", value!(vec![value])),
                    "lyricist" => ("xesam:lyricist", value!(vec![value])),
                    "tbp" | "tbpm" | "bpm" => ("xesam:audioBPM", value!(integer())),
                    "disc" => ("xesam:discNumber", value!(integer())),
                    "track" => ("xesam:trackNumber", value!(integer())),
                    lyrics if lyrics.strip_prefix("lyrics").is_some() => {
                        ("xesam:asText", value!(value))
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
            ((get!(self, "duration\0", f64)? * 1E6) as i64).into(),
        );

        let path = get!(self, "path\0").unwrap_or_default();
        if let Some(url) = Url::parse(&path).ok().or_else(|| {
            get!(self, "working-directory\0")
                .and_then(|dir| Url::from_file_path(Path::new(&dir).join(&path)).ok())
        }) {
            m.insert("mpris:url", value!(url.as_str()));
        }

        if let Some(url) = thumb.await {
            m.insert("mpris:artUrl", value!(url));
        }

        Ok(m)
    }

    /// Volume property
    #[dbus_interface(property)]
    fn volume(self) -> Result<f64> {
        Ok(get!(self, "volume\0", f64)? / 100.0)
    }

    #[dbus_interface(property)]
    fn set_volume(self, value: f64) {
        _ = set!(self, "volume\0", f64, value * 100.0);
    }

    /// Position property
    #[dbus_interface(property)]
    fn position(self) -> Result<i64> {
        Ok((get!(self, "playback-time\0", f64)? * 1E6) as i64)
    }

    // SetPosition method
    fn set_position(self, track_id: zvariant::ObjectPath<'_>, position: i64) {
        _ = track_id;
        _ = set!(self, "playback-time\0", f64, (position as f64) / 1E6);
    }
}

type Error = zbus::fdo::Error;
type Result<T = (), E = Error> = result::Result<T, E>;
