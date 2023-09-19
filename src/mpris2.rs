use std::{
    collections::{hash_map, HashMap},
    env,
    fs::File,
    io::{self, BufRead, BufReader},
    path::Path,
    time::Duration,
};

use data_encoding::BASE64;
use smol::{future::FutureExt, process::Command, Timer};
use url::Url;
use zbus::{
    self, dbus_interface, fdo,
    zvariant::{ObjectPath, Value},
};

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Root(pub crate::Handle);

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Player(pub crate::Handle);

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

pub fn time_as_secs(time: i64) -> f64 {
    Duration::from_micros(time.try_into().unwrap_or(u64::MIN)).as_secs_f64()
}

pub fn time_from_secs(secs: f64) -> i64 {
    let secs = Duration::try_from_secs_f64(secs).unwrap_or(Duration::ZERO);
    secs.as_micros().try_into().unwrap_or(i64::MAX)
}

#[allow(clippy::unused_self)]
#[dbus_interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    #[dbus_interface(property)]
    fn desktop_entry(self) -> &'static str {
        "mpv"
    }

    #[dbus_interface(property)]
    fn identity(self) -> &'static str {
        "mpv Media Player"
    }

    #[dbus_interface(property)]
    fn supported_mime_types(self) -> Vec<String> {
        env::var("XDG_DATA_DIRS")
            .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_owned())
            .split(':')
            .map(Path::new)
            .filter_map(|dir| {
                dir.is_absolute()
                    .then(|| dir.join("applications/mpv.desktop"))
                    .and_then(|path| File::open(path).ok())
                    .map(BufReader::new)
                    .map(BufRead::lines)
            })
            .flatten()
            .filter_map(Result::ok)
            .find_map(|line| line.strip_prefix("MimeType=").map(str::to_owned))
            .map_or_else(Vec::new, |v| {
                v.split_terminator(';').map(str::to_owned).collect()
            })
    }

    #[dbus_interface(property)]
    fn supported_uri_schemes(self) -> fdo::Result<Vec<String>> {
        get!(self, "protocol-list")
            .ok_or_else(|| fdo::Error::Failed("cannot get protocol-list".into()))
            .map(|x| x.split(',').map(str::to_owned).collect())
    }

    #[dbus_interface(property)]
    fn can_quit(self) -> bool {
        true
    }

    fn quit(self) -> fdo::Result<()> {
        Ok(command!(self, "quit")?)
    }

    #[dbus_interface(property)]
    fn can_raise(self) -> bool {
        false
    }

    //fn raise(self) {}

    #[dbus_interface(property)]
    fn can_set_fullscreen(self) -> bool {
        true
    }

    #[dbus_interface(property)]
    fn fullscreen(self) -> fdo::Result<bool> {
        Ok(get!(self, "fullscreen", bool)?)
    }

    #[dbus_interface(property)]
    fn set_fullscreen(self, fullscreen: bool) -> zbus::Result<()> {
        Ok(set!(self, "fullscreen", bool, fullscreen)?)
    }

    #[dbus_interface(property)]
    fn has_track_list(self) -> bool {
        false
    }
}

#[allow(clippy::unused_self)]
#[dbus_interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    #[dbus_interface(property)]
    fn can_go_next(self) -> bool {
        true
    }

    fn next(self) -> fdo::Result<()> {
        Ok(command!(self, "playlist-next")?)
    }

    #[dbus_interface(property)]
    fn can_go_previous(self) -> bool {
        true
    }

    fn previous(self) -> fdo::Result<()> {
        Ok(command!(self, "playlist-prev")?)
    }

    #[dbus_interface(property)]
    fn can_pause(self) -> bool {
        true
    }

    fn pause(self) -> fdo::Result<()> {
        Ok(set!(self, "pause", bool, true)?)
    }

    fn play_pause(self) -> fdo::Result<()> {
        Ok(command!(self, "cycle", "pause")?)
    }

    #[dbus_interface(property)]
    fn can_play(self) -> bool {
        true
    }

    fn play(self) -> fdo::Result<()> {
        Ok(set!(self, "pause", bool, false)?)
    }

    #[dbus_interface(property)]
    fn can_seek(self) -> fdo::Result<bool> {
        Ok(get!(self, "seekable", bool)?)
    }

    fn seek(self, offset: i64) -> fdo::Result<()> {
        let offset = format!("{}\0", time_as_secs(offset));
        Ok(command!(self, "seek", offset.as_str())?)
    }

    #[dbus_interface(signal)]
    pub async fn seeked(ctxt: &zbus::SignalContext<'_>, position: i64) -> zbus::Result<()>;

    fn open_uri(self, uri: String) -> fdo::Result<()> {
        let uri = uri + "\0";
        Ok(command!(self, "loadfile", uri.as_str())?)
    }

    #[dbus_interface(property)]
    fn can_control(self) -> bool {
        true
    }

    fn stop(self) -> fdo::Result<()> {
        Ok(command!(self, "stop")?)
    }

    #[dbus_interface(property)]
    fn playback_status(self) -> fdo::Result<&'static str> {
        playback_status_from(self, None, None, None)
    }

    #[dbus_interface(property)]
    fn loop_status(self) -> fdo::Result<&'static str> {
        loop_status_from(self, None, None)
    }

    #[dbus_interface(property)]
    fn set_loop_status(self, loop_status: &str) -> zbus::Result<()> {
        set!(
            self,
            "loop-file",
            match loop_status {
                "Track" => "inf",
                _ => "no",
            }
        )?;
        set!(
            self,
            "loop-playlist",
            match loop_status {
                "Playlist" => "inf",
                _ => "no",
            }
        )?;
        Ok(())
    }

    #[dbus_interface(property)]
    fn rate(self) -> fdo::Result<f64> {
        Ok(get!(self, "speed", f64)?)
    }

    #[dbus_interface(property)]
    fn set_rate(self, rate: f64) -> zbus::Result<()> {
        Ok(set!(self, "speed", f64, rate)?)
    }

    #[dbus_interface(property)]
    fn minimum_rate(self) -> fdo::Result<f64> {
        Ok(get!(self, "option-info/speed/min", f64)?)
    }

    #[dbus_interface(property)]
    fn maximum_rate(self) -> fdo::Result<f64> {
        Ok(get!(self, "option-info/speed/max", f64)?)
    }

    #[dbus_interface(property)]
    fn shuffle(self) -> fdo::Result<bool> {
        Ok(get!(self, "shuffle", bool)?)
    }

    #[dbus_interface(property)]
    fn set_shuffle(self, shuffle: bool) -> zbus::Result<()> {
        Ok(set!(self, "shuffle", bool, shuffle)?)
    }

    #[dbus_interface(property)]
    pub async fn metadata(self) -> fdo::Result<HashMap<&'static str, Value<'static>>> {
        metadata(self).await
    }

    #[dbus_interface(property)]
    fn volume(self) -> fdo::Result<f64> {
        Ok(get!(self, "volume", f64)? / 100.0)
    }

    #[dbus_interface(property)]
    fn set_volume(self, volume: f64) -> zbus::Result<()> {
        Ok(set!(self, "volume", f64, volume * 100.0)?)
    }

    #[dbus_interface(property)]
    fn position(self) -> fdo::Result<i64> {
        Ok(time_from_secs(get!(self, "playback-time", f64)?))
    }

    #[allow(clippy::needless_pass_by_value)]
    fn set_position(self, track_id: ObjectPath, position: i64) -> fdo::Result<()> {
        _ = track_id;
        Ok(set!(self, "playback-time", f64, time_as_secs(position))?)
    }
}

pub fn playback_status_from(
    ctx: impl Into<*mut crate::mpv_handle> + Copy,
    idle_active: Option<bool>,
    eof_reached: Option<bool>,
    pause: Option<bool>,
) -> fdo::Result<&'static str> {
    let (idle_active, eof_reached) = (idle_active.ok_or(()), eof_reached.ok_or(()));
    if idle_active.or_else(|()| get!(ctx, "idle-active", bool))?
        || eof_reached.or_else(|()| get!(ctx, "eof-reached", bool))?
    {
        Ok("Stopped")
    } else if pause.ok_or(()).or_else(|_| get!(ctx, "pause", bool))? {
        Ok("Paused")
    } else {
        Ok("Playing")
    }
}

pub fn loop_status_from(
    ctx: impl Into<*mut crate::mpv_handle> + Copy,
    loop_file: Option<String>,
    loop_playlist: Option<String>,
) -> fdo::Result<&'static str> {
    let err = || fdo::Error::Failed("cannot get property".into());
    if loop_file.ok_or_else(err)? == "no" || get!(ctx, "list-file").ok_or_else(err)? == "no" {
        Ok("Track")
    } else if loop_playlist.ok_or_else(err)? == "no"
        || get!(ctx, "list-playlist").ok_or_else(err)? == "no"
    {
        Ok("Playlist")
    } else {
        Ok("None")
    }
}

pub async fn metadata(
    ctx: impl Into<*mut crate::mpv_handle> + Copy + Send + 'static,
) -> fdo::Result<HashMap<&'static str, Value<'static>>> {
    let thumb = smol::spawn(async move {
        let path = get!(ctx, "path").unwrap_or_default();
        if path == get!(ctx, "stream-open-filename").unwrap_or_default() {
            Command::new("ffmpegthumbnailer")
                .args(["-m", "-cjpeg", "-s0", "-o-", "-i"])
                .arg(path)
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
                        .arg(path)
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

    m.insert(
        "mpris:length",
        time_from_secs(get!(ctx, "duration", f64)?).into(),
    );

    if let Some(data) = get!(ctx, "metadata") {
        let data: HashMap<&str, String> =
            serde_json::from_str(data).map_err(|err| fdo::Error::Failed(err.to_string()))?;
        for (key, value) in data {
            let integer = || -> i64 {
                value
                    .find('/')
                    .map_or_else(|| &value[..], |x| &value[..x])
                    .parse()
                    .unwrap_or_default()
            };
            let (key, value): (_, Value<'_>) = match key.to_ascii_lowercase().as_str() {
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
                lyrics if lyrics.strip_prefix("lyrics").is_some() => ("xesam:asText", value.into()),
                _ => continue,
            };
            m.insert(key, value);
        }
    }

    if let hash_map::Entry::Vacant(v) = m.entry("xesam:title") {
        if let Some(s) = get!(ctx, "media-title") {
            v.insert(String::from(s).into());
        }
    }

    m.insert(
        "mpris:trackid",
        ObjectPath::try_from("/io/mpv")
            .map_err(zbus::Error::from)?
            .into(),
    );

    let path = get!(ctx, "path").unwrap_or_default();
    if let Some(url) = Url::parse(path).ok().or_else(|| {
        get!(ctx, "working-directory")
            .and_then(|dir| Url::from_file_path(Path::new(dir).join(path)).ok())
    }) {
        m.insert("mpris:url", String::from(url).into());
    }

    if let Some(url) = thumb.await {
        m.insert("mpris:artUrl", url.into());
    }

    Ok(m)
}
