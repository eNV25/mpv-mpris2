#![allow(clippy::ignored_unit_patterns)] // for interface macro

use std::{
    collections::HashMap,
    env,
    fs::File,
    io::{self, BufRead, BufReader},
    path::{Path, PathBuf},
    time::Duration,
};

use data_encoding::BASE64;
use smol::{future::FutureExt, process::Command, Timer};
use url::Url;
use zbus::{
    fdo, interface,
    object_server::SignalEmitter,
    zvariant::{ObjectPath, Value},
};

use crate::Block;

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Root(pub crate::MPVHandle);

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Player(pub crate::MPVHandle);

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
#[interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    #[zbus(property)]
    fn desktop_entry(self) -> &'static str {
        "mpv"
    }

    #[zbus(property)]
    fn identity(self) -> &'static str {
        "mpv Media Player"
    }

    #[zbus(property)]
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
            .find_map(|line| {
                line.strip_prefix("MimeType=")
                    .map(|v| v.split_terminator(';').map(str::to_owned).collect())
            })
            .unwrap_or_default()
    }

    #[zbus(property)]
    fn supported_uri_schemes(self) -> Vec<String> {
        get!(self, c"protocol-list")
            .split(',')
            .map(str::to_owned)
            .collect()
    }

    #[zbus(property)]
    fn can_quit(self) -> bool {
        true
    }

    fn quit(self) -> fdo::Result<()> {
        Ok(command!(self, c"quit")?)
    }

    #[zbus(property)]
    fn can_raise(self) -> bool {
        false
    }

    //fn raise(self) {}

    #[zbus(property)]
    fn can_set_fullscreen(self) -> bool {
        true
    }

    #[zbus(property)]
    fn fullscreen(self) -> fdo::Result<bool> {
        Ok(get!(self, c"fullscreen", bool)?)
    }

    #[zbus(property)]
    fn set_fullscreen(self, fullscreen: bool) -> zbus::Result<()> {
        Ok(set!(self, c"fullscreen", bool, fullscreen)?)
    }

    #[zbus(property)]
    fn has_track_list(self) -> bool {
        false
    }
}

#[allow(clippy::unused_self)]
#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    #[zbus(property)]
    fn can_go_next(self) -> bool {
        true
    }

    fn next(self) -> fdo::Result<()> {
        Ok(command!(self, c"playlist-next")?)
    }

    #[zbus(property)]
    fn can_go_previous(self) -> bool {
        true
    }

    fn previous(self) -> fdo::Result<()> {
        Ok(command!(self, c"playlist-prev")?)
    }

    #[zbus(property)]
    fn can_pause(self) -> bool {
        true
    }

    fn pause(self) -> fdo::Result<()> {
        Ok(set!(self, c"pause", bool, true)?)
    }

    fn play_pause(self) -> fdo::Result<()> {
        Ok(command!(self, c"cycle", c"pause")?)
    }

    #[zbus(property)]
    fn can_play(self) -> bool {
        true
    }

    fn play(self) -> fdo::Result<()> {
        Ok(set!(self, c"pause", bool, false)?)
    }

    #[zbus(property)]
    fn can_seek(self) -> fdo::Result<bool> {
        Ok(get!(self, c"seekable", bool)?)
    }

    fn seek(self, offset: i64) -> fdo::Result<()> {
        let offset = format!("{}\0", time_as_secs(offset));
        Ok(command!(self, c"seek", offset.as_str())?)
    }

    #[zbus(signal)]
    pub async fn seeked(emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;

    fn open_uri(self, mut uri: String) -> fdo::Result<()> {
        uri.push('\0');
        Ok(command!(self, c"loadfile", uri.as_str())?)
    }

    #[zbus(property)]
    fn can_control(self) -> bool {
        true
    }

    fn stop(self) -> fdo::Result<()> {
        Ok(command!(self, c"stop")?)
    }

    #[zbus(property)]
    fn playback_status(self) -> fdo::Result<&'static str> {
        playback_status_from(self.0, None, None, None)
    }

    #[zbus(property)]
    fn loop_status(self) -> &'static str {
        loop_status_from(self.0, None, None)
    }

    #[zbus(property)]
    fn set_loop_status(self, loop_status: &str) -> zbus::Result<()> {
        set!(
            self,
            c"loop-file",
            match loop_status {
                "Track" => c"inf",
                _ => c"no",
            }
        )?;
        set!(
            self,
            c"loop-playlist",
            match loop_status {
                "Playlist" => c"inf",
                _ => c"no",
            }
        )?;
        Ok(())
    }

    #[zbus(property)]
    fn rate(self) -> fdo::Result<f64> {
        Ok(get!(self, c"speed", f64)?)
    }

    #[zbus(property)]
    fn set_rate(self, rate: f64) -> zbus::Result<()> {
        Ok(set!(self, c"speed", f64, rate)?)
    }

    #[zbus(property)]
    fn minimum_rate(self) -> fdo::Result<f64> {
        Ok(get!(self, c"option-info/speed/min", f64)?)
    }

    #[zbus(property)]
    fn maximum_rate(self) -> fdo::Result<f64> {
        Ok(get!(self, c"option-info/speed/max", f64)?)
    }

    #[zbus(property)]
    fn shuffle(self) -> fdo::Result<bool> {
        Ok(get!(self, c"shuffle", bool)?)
    }

    #[zbus(property)]
    fn set_shuffle(self, shuffle: bool) -> zbus::Result<()> {
        Ok(set!(self, c"shuffle", bool, shuffle)?)
    }

    #[zbus(property)]
    pub fn metadata(self) -> HashMap<&'static str, Value<'static>> {
        metadata(self.0)
    }

    #[zbus(property)]
    fn volume(self) -> fdo::Result<f64> {
        Ok(get!(self, c"volume", f64)? / 100.0)
    }

    #[zbus(property)]
    fn set_volume(self, volume: f64) -> zbus::Result<()> {
        Ok(set!(self, c"volume", f64, volume * 100.0)?)
    }

    #[zbus(property)]
    fn position(self) -> fdo::Result<i64> {
        Ok(time_from_secs(get!(self, c"playback-time", f64)?))
    }

    #[allow(clippy::needless_pass_by_value)]
    fn set_position(self, track_id: ObjectPath, position: i64) -> fdo::Result<()> {
        _ = track_id;
        Ok(set!(self, c"playback-time", f64, time_as_secs(position))?)
    }
}

pub fn playback_status_from(
    mpv: crate::MPVHandle,
    idle_active: Option<bool>,
    eof_reached: Option<bool>,
    pause: Option<bool>,
) -> fdo::Result<&'static str> {
    let idle_active = idle_active.ok_or(());
    if idle_active.or_else(|()| get!(mpv, c"idle-active", bool))?
        || eof_reached
            .or_else(|| get!(mpv, c"eof-reached", bool).ok())
            .unwrap_or(false)
    {
        Ok("Stopped")
    } else if pause.ok_or(()).or_else(|()| get!(mpv, c"pause", bool))? {
        Ok("Paused")
    } else {
        Ok("Playing")
    }
}

pub fn loop_status_from(
    mpv: crate::MPVHandle,
    loop_file: Option<bool>,
    loop_playlist: Option<bool>,
) -> &'static str {
    let loop_file = loop_file.unwrap_or_else(|| get!(mpv, c"loop-file") != "no");
    let loop_playlist = loop_playlist.unwrap_or_else(|| get!(mpv, c"loop-playlist") != "no");
    if loop_file {
        "Track"
    } else if loop_playlist {
        "Playlist"
    } else {
        "None"
    }
}

pub fn metadata(mpv: crate::MPVHandle) -> HashMap<&'static str, Value<'static>> {
    const TRACK_ID: Value<'static> =
        Value::ObjectPath(ObjectPath::from_static_str_unchecked("/io/mpv"));

    let mut m = HashMap::new();
    m.insert("mpris:trackid", TRACK_ID);

    if let Ok(duration) = get!(mpv, c"duration", f64) {
        m.insert("mpris:length", time_from_secs(duration).into());
    }

    let path = get!(mpv, c"path");
    if Url::parse(&path).is_ok() {
        m.insert("xesam:url", path.into());
    } else {
        let mut file = PathBuf::from(get!(mpv, c"working-directory"));
        file.push(path);
        if let Ok(uri) = Url::from_file_path(file) {
            m.insert("xesam:url", String::from(uri).into());
        }
    }

    let data = get!(mpv, c"metadata");
    if let Ok(data) = serde_json::from_str::<HashMap<&str, String>>(&data) {
        for (key, value) in data {
            let integer = || -> i32 {
                value
                    .find('/')
                    .map_or_else(|| &value[..], |x| &value[..x])
                    .parse()
                    .unwrap_or_default()
            };
            let (key, value) = match key.to_ascii_lowercase().as_str() {
                "album" => ("xesam:album", value.into()),
                //"title" => ("xesam:title", value.into()),
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

    m.insert("xesam:title", get!(mpv, c"media-title").into());

    if let Some(url) = thumbnail(mpv) {
        m.insert("mpris:artUrl", url.into());
    }

    m
}

fn thumbnail(mpv: crate::MPVHandle) -> Option<String> {
    let path = get!(mpv, c"path");
    if path == get!(mpv, c"stream-open-filename") {
        Command::new("ffmpegthumbnailer")
            .args(["-m", "-cjpeg", "-s0", "-o-", "-i"])
            .arg(&path)
            .kill_on_drop(true)
            .output()
            .or(async {
                Timer::after(Duration::from_secs(1)).await;
                Err(io::ErrorKind::TimedOut.into())
            })
            .block()
            .ok()
            .map(|output| {
                const PREFIX: &str = "data:image/jpeg;base64,";
                let len = PREFIX.len() + BASE64.encode_len(output.stdout.len());
                let mut data = String::with_capacity(len);
                data.push_str(PREFIX);
                BASE64.encode_append(&output.stdout, &mut data);
                data
            })
    } else {
        ["yt-dlp", "yt-dlp_x86", "youtube-dl"]
            .into_iter()
            .find_map(|cmd| {
                Command::new(cmd)
                    .args(["--no-warnings", "--get-thumbnail"])
                    .arg(&path)
                    .kill_on_drop(true)
                    .output()
                    .or(async {
                        Timer::after(Duration::from_secs(5)).await;
                        Err(io::ErrorKind::TimedOut.into())
                    })
                    .block()
                    .ok()
                    .map(|output| String::from(String::from_utf8_lossy(&output.stdout)))
                    .map(truncate_newline)
            })
    }
    .filter(|url| Url::parse(url).is_ok())
}

fn truncate_newline(mut s: String) -> String {
    if let [.., r, b'\n'] = s.as_bytes() {
        if let b'\r' = r {
            s.truncate(s.len() - 2);
        } else {
            s.truncate(s.len() - 1);
        }
    }
    s
}
