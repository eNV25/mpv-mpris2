use std::{
    collections::HashMap,
    env,
    fs::File,
    io::{self, BufRead, BufReader},
    path::Path,
    time::Duration,
};

use data_encoding::BASE64;
use mpris_server::{
    async_trait,
    zbus::{
        self, fdo,
        zvariant::{ObjectPath, Value},
    },
    LoopStatus, Metadata, PlaybackRate, PlaybackStatus, PlayerInterface, RootInterface, Time,
    TrackId, Volume,
};
use smol::{future::FutureExt, process::Command, Timer};
use url::Url;

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Player(pub crate::Handle);

impl From<Player> for *mut crate::mpv_handle {
    #[inline]
    fn from(value: Player) -> Self {
        value.0 .0
    }
}

impl From<&Player> for *mut crate::mpv_handle {
    #[inline]
    fn from(value: &Player) -> Self {
        value.0 .0
    }
}

pub fn time_as_secs(time: Time) -> f64 {
    Duration::from_micros(time.as_micros().try_into().unwrap_or(u64::MIN)).as_secs_f64()
}

pub fn time_from_secs(secs: f64) -> Time {
    let secs = Duration::try_from_secs_f64(secs).unwrap_or(Duration::ZERO);
    Time::from_micros(secs.as_micros().try_into().unwrap_or(i64::MAX))
}

#[async_trait]
impl RootInterface for Player {
    async fn desktop_entry(&self) -> fdo::Result<String> {
        Ok("mpv".into())
    }

    async fn identity(&self) -> fdo::Result<String> {
        Ok("mpv Media Player".into())
    }

    async fn supported_mime_types(&self) -> fdo::Result<Vec<String>> {
        Ok(env::var("XDG_DATA_DIRS")
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
            }))
    }

    async fn supported_uri_schemes(&self) -> fdo::Result<Vec<String>> {
        get!(self, "protocol-list")
            .ok_or_else(|| fdo::Error::Failed("cannot get protocol-list".into()))
            .map(|x| x.split(',').map(str::to_owned).collect())
    }

    async fn can_quit(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn quit(&self) -> fdo::Result<()> {
        Ok(command!(self, "quit")?)
    }

    async fn can_raise(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn raise(&self) -> fdo::Result<()> {
        Err(fdo::Error::NotSupported(
            "Unsupported method 'Raise'".into(),
        ))
    }

    async fn can_set_fullscreen(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn fullscreen(&self) -> fdo::Result<bool> {
        Ok(get!(self, "fullscreen", bool)?)
    }

    async fn set_fullscreen(&self, fullscreen: bool) -> zbus::Result<()> {
        Ok(set!(self, "fullscreen", bool, fullscreen)?)
    }

    async fn has_track_list(&self) -> fdo::Result<bool> {
        Ok(false)
    }
}

#[async_trait]
impl PlayerInterface for Player {
    async fn can_go_next(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn next(&self) -> fdo::Result<()> {
        Ok(command!(self, "playlist-next")?)
    }

    async fn can_go_previous(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn previous(&self) -> fdo::Result<()> {
        Ok(command!(self, "playlist-prev")?)
    }

    async fn can_pause(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn pause(&self) -> fdo::Result<()> {
        Ok(set!(self, "pause", bool, true)?)
    }

    async fn play_pause(&self) -> fdo::Result<()> {
        Ok(command!(self, "cycle", "pause")?)
    }

    async fn can_play(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn play(&self) -> fdo::Result<()> {
        Ok(set!(self, "pause", bool, false)?)
    }

    async fn can_seek(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn seek(&self, offset: Time) -> fdo::Result<()> {
        let offset = format!("{}\0", time_as_secs(offset));
        Ok(command!(self, "seek", offset.as_str())?)
    }

    async fn open_uri(&self, uri: String) -> fdo::Result<()> {
        let uri = uri + "\0";
        Ok(command!(self, "loadfile", uri.as_str())?)
    }

    async fn can_control(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn stop(&self) -> fdo::Result<()> {
        Ok(command!(self, "stop")?)
    }

    async fn playback_status(&self) -> fdo::Result<PlaybackStatus> {
        if get!(self, "idle-active", bool)? || get!(self, "eof-reached", bool)? {
            Ok(PlaybackStatus::Stopped)
        } else if get!(self, "pause", bool)? {
            Ok(PlaybackStatus::Paused)
        } else {
            Ok(PlaybackStatus::Playing)
        }
    }

    async fn loop_status(&self) -> fdo::Result<LoopStatus> {
        let err = || fdo::Error::Failed("cannot get property".into());
        if get!(self, "loop-file").ok_or_else(err)? != "no" {
            Ok(LoopStatus::Track)
        } else if get!(self, "loop-playlist").ok_or_else(err)? != "no" {
            Ok(LoopStatus::Playlist)
        } else {
            Ok(LoopStatus::None)
        }
    }

    async fn set_loop_status(&self, loop_status: LoopStatus) -> zbus::Result<()> {
        set!(
            self,
            "loop-file",
            match loop_status {
                LoopStatus::Track => "inf",
                _ => "no",
            }
        )?;
        set!(
            self,
            "loop-playlist",
            match loop_status {
                LoopStatus::Playlist => "inf",
                _ => "no",
            }
        )?;
        Ok(())
    }

    async fn rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(get!(self, "speed", f64)?)
    }

    async fn set_rate(&self, rate: PlaybackRate) -> zbus::Result<()> {
        Ok(set!(self, "speed", f64, rate)?)
    }

    async fn minimum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(get!(self, "option-info/speed/min", f64)?)
    }

    async fn maximum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(get!(self, "option-info/speed/max", f64)?)
    }

    async fn shuffle(&self) -> fdo::Result<bool> {
        Ok(get!(self, "shuffle", bool)?)
    }

    async fn set_shuffle(&self, shuffle: bool) -> zbus::Result<()> {
        Ok(set!(self, "shuffle", bool, shuffle)?)
    }

    async fn metadata(&self) -> fdo::Result<Metadata> {
        let self = *self;

        let thumb = smol::spawn(async move {
            let path = get!(self, "path").unwrap_or_default();
            if path == get!(self, "stream-open-filename").unwrap_or_default() {
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

        let mut m = Metadata::new();

        m.insert("mpris:length", time_from_secs(get!(self, "duration", f64)?));

        if let Some(s) = get!(self, "media-title") {
            m.insert("xesam:title", s);
        }

        if let Some(data) = get!(self, "metadata") {
            let data: HashMap<&str, String> =
                serde_json::from_str(&data).map_err(|err| fdo::Error::Failed(err.to_string()))?;
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
            ObjectPath::try_from("/io/mpv").map_err(zbus::Error::from)?,
        );

        let path = get!(self, "path").unwrap_or_default();
        if let Some(url) = Url::parse(&path).ok().or_else(|| {
            get!(self, "working-directory")
                .and_then(|dir| Url::from_file_path(Path::new(&dir).join(&path)).ok())
        }) {
            m.insert("mpris:url", String::from(url));
        }

        if let Some(url) = thumb.await {
            m.insert("mpris:artUrl", url);
        }

        Ok(m)
    }

    async fn volume(&self) -> fdo::Result<Volume> {
        Ok(get!(self, "volume", f64)? / 100.0)
    }

    async fn set_volume(&self, volume: Volume) -> zbus::Result<()> {
        Ok(set!(self, "volume", f64, volume * 100.0)?)
    }

    async fn position(&self) -> fdo::Result<Time> {
        Ok(time_from_secs(get!(self, "playback-time", f64)?))
    }

    async fn set_position(&self, _: TrackId, position: Time) -> fdo::Result<()> {
        Ok(set!(self, "playback-time", f64, time_as_secs(position))?)
    }
}
