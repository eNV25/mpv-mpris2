use std::{collections::HashMap, path::Path};

use data_encoding::BASE64;
use smol::process::Command;
use url::Url;
use zbus::dbus_interface;

use crate::mpv;

#[repr(transparent)]
pub struct PlayerProxy {
    ctx: mpv::Handle,
}

impl From<*mut mpv::capi::mpv_handle> for PlayerProxy {
    fn from(value: *mut mpv::capi::mpv_handle) -> Self {
        Self {
            ctx: mpv::Handle(value),
        }
    }
}

impl PlayerProxy {
    #[inline(always)]
    fn ctx(&self) -> *mut crate::mpv::capi::mpv_handle {
        self.ctx.0
    }
}

#[dbus_interface(name = "org.mpris.MediaPlayer2.Player")]
impl PlayerProxy {
    /// CanGoNext property
    #[dbus_interface(property)]
    fn can_go_next(&self) -> bool {
        true
    }

    /// Next method
    fn next(&self) {
        mpv::command!(self.ctx(), "playlist-next\0");
    }

    /// CanGoPrevious property
    #[dbus_interface(property)]
    fn can_go_previous(&self) -> bool {
        true
    }

    /// Previous method
    fn previous(&self) {
        mpv::command!(self.ctx(), "playlist-prev\0");
    }

    /// CanPause property
    #[dbus_interface(property)]
    fn can_pause(&self) -> bool {
        true
    }

    /// Pause method
    fn pause(&self) {
        mpv::set_property_bool!(self.ctx(), "pause\0", true);
    }

    /// PlayPause method
    fn play_pause(&self) {
        mpv::command!(self.ctx(), "cycle\0", "pause\0");
    }

    /// CanPlay property
    #[dbus_interface(property)]
    fn can_play(&self) -> bool {
        true
    }

    /// Play method
    fn play(&self) {
        mpv::set_property_bool!(self.ctx(), "pause\0", false);
    }

    /// CanSeek property
    #[dbus_interface(property)]
    fn can_seek(&self) -> bool {
        mpv::get_property_bool!(self.ctx(), "seekable\0")
    }

    /// Seek method
    fn seek(&self, offset: i64) {
        mpv::command!(self.ctx(), "seek\0", format!("{}\0", (offset as f64) / 1E6));
    }

    /// Seeked signal
    #[dbus_interface(signal)]
    pub async fn seeked(ctxt: &zbus::SignalContext<'_>, position: i64) -> zbus::Result<()>;

    // OpenUri method
    fn open_uri(&self, uri: &str) {
        mpv::command!(self.ctx(), "loadfile\0", format!("{}\0", uri));
    }

    /// CanControl property
    #[dbus_interface(property)]
    fn can_control(&self) -> bool {
        true
    }

    /// Stop method
    fn stop(&self) {
        mpv::command!(self.ctx(), "stop\0");
    }

    /// PlaybackStatus property
    #[dbus_interface(property)]
    fn playback_status(&self) -> &str {
        if mpv::get_property_bool!(self.ctx(), "idle-active\0")
            || (mpv::get_property_string!(self.ctx(), "keep-open\0") != "no"
                && (mpv::get_property_int!(self.ctx(), "percent-pos\0") == 100
                    || mpv::get_property_int!(self.ctx(), "duration\0") == 0))
        {
            "Stopped"
        } else if mpv::get_property_bool!(self.ctx(), "pause\0") {
            "Paused"
        } else {
            "Playing"
        }
    }

    /// LoopStatus property
    #[dbus_interface(property)]
    fn loop_status(&self) -> &str {
        if mpv::get_property_string!(self.ctx(), "loop-file\0") != "no" {
            "Track"
        } else if mpv::get_property_string!(self.ctx(), "loop-playlist\0") != "no" {
            "Playlist"
        } else {
            "None"
        }
    }

    #[dbus_interface(property)]
    fn set_loop_status(&self, value: &str) {
        mpv::set_property_string!(
            self.ctx(),
            "loop-file\0",
            match value {
                "Track" => "inf\0",
                _ => "no\0",
            }
        );
        mpv::set_property_string!(
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
        mpv::get_property_float!(self.ctx(), "speed\0")
    }

    #[dbus_interface(property)]
    fn set_rate(&self, value: f64) {
        mpv::set_property_float!(self.ctx(), "speed\0", value);
    }

    /// MinimumRate property
    #[dbus_interface(property)]
    fn minimum_rate(&self) -> f64 {
        mpv::get_property_float!(self.ctx(), "option-info/speed/min\0")
    }

    /// MaximumRate property
    #[dbus_interface(property)]
    fn maximum_rate(&self) -> f64 {
        mpv::get_property_float!(self.ctx(), "option-info/speed/max\0")
    }

    /// Shuffle property
    #[dbus_interface(property)]
    fn shuffle(&self) -> bool {
        mpv::get_property_bool!(self.ctx(), "shuffle\0")
    }

    #[dbus_interface(property)]
    fn set_shuffle(&self, value: bool) {
        mpv::set_property_bool!(self.ctx(), "shuffle\0", value);
    }

    /// Metadata property
    #[dbus_interface(property)]
    async fn metadata(&self) -> HashMap<String, zbus::zvariant::Value> {
        let mut metadata = HashMap::new();

        let track = match mpv::get_property_int!(self.ctx(), "playlist-playing-pos\0") {
            pos if pos.is_negative() => "/io/mpv/noplaylist".into(),
            pos => format!("/io/mpv/playlist/{}", pos),
        };
        _ = zbus::zvariant::ObjectPath::try_from(track).map(|path| {
            metadata.insert("mpris:trackid".into(), path.into());
        });

        metadata.insert(
            "mpris:length".into(),
            ((mpv::get_property_float!(self.ctx(), "duration\0") * 1E6) as i64).into(),
        );

        let path = mpv::get_property_string!(self.ctx(), "path\0");
        let stream = mpv::get_property_string!(self.ctx(), "stream-open-filename\0");

        _ = Url::parse(path)
            .or_else(|_| {
                Url::from_file_path(
                    Path::new(mpv::get_property_string!(self.ctx(), "working-directory\0"))
                        .join(path),
                )
            })
            .map(|url| {
                metadata.insert("mpris:url".into(), url.as_str().to_owned().into());
            });

        if path == stream {
            _ = Command::new("ffmpegthumbnailer")
                .args(&["-m", "-cjpeg", "-s0", "-o-", "-i"])
                .arg(stream)
                .output()
                .await
                .map(|output| BASE64.encode(&output.stdout))
                .map(|data| format!("data:image/jpeg;base64,{data}"))
                .map(|url| {
                    metadata.insert("mpris:artUrl".into(), url.into());
                });
        } else {
            for cmd in ["yt-dlp", "yt-dlp_x86", "youtube-dl"] {
                if let Some(..) = Command::new(cmd)
                    .arg("--get-thumbnail")
                    .arg(path)
                    .output()
                    .await
                    .ok()
                    .and_then(|output| {
                        std::str::from_utf8(&output.stdout)
                            .map(|s| s.trim_matches(char::is_whitespace).to_owned())
                            .ok()
                    })
                    .map(|url| {
                        metadata.insert("mpris:artUrl".into(), url.into());
                    })
                {
                    break;
                }
            }
        }

        macro_rules! add {
            ($metadata:expr, $key:expr, $mpvkey:expr) => {
                if let Some(value) =
                    Some(mpv::get_property_string!(self.ctx(), $mpvkey)).filter(|s| !s.is_empty())
                {
                    $metadata.insert($key.into(), value.to_owned().into());
                }
            };
        }

        add!(metadata, "xesam:title", "media-title\0");
        add!(metadata, "xesam:title", "metadata/by-key/Title\0");
        add!(metadata, "xesam:album", "metadata/by-key/Album\0");
        add!(metadata, "xesam:genre", "metadata/by-key/Genre\0");

        add!(metadata, "xesam:artist", "metadata/by-key/uploader\0");
        add!(metadata, "xesam:artist", "metadata/by-key/Artist\0");
        add!(
            metadata,
            "xesam:albumArtist",
            "metadata/by-key/Album_Artist\0"
        );
        add!(metadata, "xesam:composer", "metadata/by-key/Composer\0");

        add!(metadata, "xesam:trackNumber", "metadata/by-key/Track\0");
        add!(metadata, "xesam:discNumber", "metadata/by-key/Disc\0");

        add!(
            metadata,
            "mb:artistId",
            "metadata/by-key/MusicBrainz Artist Id\0"
        );
        add!(
            metadata,
            "mb:trackId",
            "metadata/by-key/MusicBrainz Track Id\0"
        );
        add!(
            metadata,
            "mb:albumArtistId",
            "metadata/by-key/MusicBrainz Album Artist Id\0"
        );
        add!(
            metadata,
            "mb:albumId",
            "metadata/by-key/MusicBrainz Album Id\0"
        );
        add!(
            metadata,
            "mb:releaseTrackId",
            "metadata/by-key/MusicBrainz Release Track Id\0"
        );
        add!(
            metadata,
            "mb:workId",
            "metadata/by-key/MusicBrainz Work Id\0"
        );

        add!(
            metadata,
            "mb:artistId",
            "metadata/by-key/MUSICBRAINZ_ARTISTID\0"
        );
        add!(
            metadata,
            "mb:trackId",
            "metadata/by-key/MUSICBRAINZ_TRACKID\0"
        );
        add!(
            metadata,
            "mb:albumArtistId",
            "metadata/by-key/MUSICBRAINZ_ALBUMARTISTID\0"
        );
        add!(
            metadata,
            "mb:albumId",
            "metadata/by-key/MUSICBRAINZ_ALBUMID\0"
        );
        add!(
            metadata,
            "mb:releaseTrackId",
            "metadata/by-key/MUSICBRAINZ_RELEASETRACKID\0"
        );
        add!(
            metadata,
            "mb:workId",
            "metadata/by-key/MUSICBRAINZ_WORKID\0"
        );

        metadata
    }

    /// Volume property
    #[dbus_interface(property)]
    fn volume(&self) -> f64 {
        mpv::get_property_float!(self.ctx(), "volume\0") / 100.0
    }

    #[dbus_interface(property)]
    fn set_volume(&self, value: f64) {
        mpv::set_property_float!(self.ctx(), "volume\0", value * 100.0)
    }

    /// Position property
    #[dbus_interface(property)]
    fn position(&self) -> i64 {
        (mpv::get_property_float!(self.ctx(), "playback-time\0") * 1E6) as i64
    }

    // SetPosition method
    fn set_position(&self, track_id: zbus::zvariant::ObjectPath<'_>, position: i64) {
        _ = track_id
            .as_str()
            .strip_prefix("/io/mpv/playlist/")
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|&track| track == mpv::get_property_int!(self.ctx(), "playlist-playing-pos\0"))
            .map(|_| {
                mpv::set_property_float!(self.ctx(), "playback-time\0", (position as f64) / 1E6);
            });
    }
}
