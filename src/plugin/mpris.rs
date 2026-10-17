use super::{art, state::StateDiff};
use crate::{future::FutureSyncExt, mpv};
use mpris_server::{
    LoopStatus, Metadata, PlaybackRate, PlaybackStatus, PlayerInterface, RootInterface, Time,
    TrackId, Volume, builder::MetadataBuilder,
};
use serde::{Deserialize, Serialize};
use smol::lock::{OnceCell, RwLockWriteGuard};
use std::path::PathBuf;
use std::{borrow::Cow, collections::BTreeMap, env, fs, mem, sync::OnceLock};
use url::Url;
use zbus::{fdo, names::InterfaceName, object_server::Interface, zvariant, zvariant::ObjectPath};

impl RootInterface for super::Player {
    async fn raise(&self) -> fdo::Result<()> {
        Ok(())
    }

    async fn quit(&self) -> fdo::Result<()> {
        let cmd = mpv::NamedCommand::Quit { code: None };
        Ok(self.mpv.command(cmd).sync().await?)
    }

    async fn can_quit(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn fullscreen(&self) -> fdo::Result<bool> {
        Ok(self.state.read().await.fullscreen)
    }

    async fn set_fullscreen(&self, value: bool) -> zbus::Result<()> {
        Ok(self.mpv.set_property("fullscreen", value).sync().await?)
    }

    async fn can_set_fullscreen(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_raise(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn has_track_list(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn identity(&self) -> fdo::Result<String> {
        Ok("mpv Media Player".into())
    }

    async fn desktop_entry(&self) -> fdo::Result<String> {
        Ok("mpv".into())
    }

    async fn supported_uri_schemes(&self) -> fdo::Result<Vec<String>> {
        static SUPPORTED_URI_SCHEMES: OnceCell<Vec<String>> = OnceCell::new();
        let schemes = SUPPORTED_URI_SCHEMES
            .get_or_try_init(|| async {
                // A list, not a comma-separated string. Some entries repeat.
                let mut schemes: Vec<String> =
                    self.mpv.get_property("protocol-list").sync().await?;
                schemes.sort_unstable();
                schemes.dedup();
                Ok::<_, mpv::Error>(schemes)
            })
            .await?;
        Ok(schemes.clone())
    }

    async fn supported_mime_types(&self) -> fdo::Result<Vec<String>> {
        static MIME_TYPES: OnceLock<Vec<String>> = OnceLock::new();
        Ok(MIME_TYPES.get_or_init(mpv_desktop_mime_types).clone())
    }
}

/// Reads the `MimeType` list from mpv's desktop entry.
fn mpv_desktop_mime_types() -> Vec<String> {
    let data_home = env::var_os("XDG_DATA_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    let data_dirs = env::var_os("XDG_DATA_DIRS")
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    data_home
        .into_iter()
        .chain(env::split_paths(&data_dirs))
        .filter_map(|dir| fs::read_to_string(dir.join("applications/mpv.desktop")).ok())
        .find_map(|entry| {
            entry.lines().find_map(|line| {
                let types = line.strip_prefix("MimeType=")?;
                Some(types.split_terminator(';').map(str::to_owned).collect())
            })
        })
        .unwrap_or_else(|| {
            tracing::error!("Failed to find MimeType in mpv.desktop");
            Vec::new()
        })
}

impl PlayerInterface for super::Player {
    async fn next(&self) -> fdo::Result<()> {
        let cmd = mpv::NamedCommand::PlaylistNext { flags: None };
        Ok(self.mpv.command(cmd).sync().await?)
    }

    async fn previous(&self) -> fdo::Result<()> {
        let cmd = mpv::NamedCommand::PlaylistPrev { flags: None };
        Ok(self.mpv.command(cmd).sync().await?)
    }

    async fn pause(&self) -> fdo::Result<()> {
        Ok(self.mpv.set_property("pause", true).sync().await?)
    }

    async fn play_pause(&self) -> fdo::Result<()> {
        let cmd = mpv::ListCommand::Cycle("pause", None);
        Ok(self.mpv.command(cmd).sync().await?)
    }

    async fn stop(&self) -> fdo::Result<()> {
        use mpv::SeekMode::*;
        self.pause().await?;
        let cmd = mpv::NamedCommand::Seek {
            target: 0.0.into(),
            flags: Some(Absolute.into()),
        };
        Ok(self.mpv.command(cmd).sync().await?)
    }

    async fn play(&self) -> fdo::Result<()> {
        Ok(self.mpv.set_property("pause", false).sync().await?)
    }

    async fn seek(&self, offset: Time) -> fdo::Result<()> {
        use mpv::{SeekMode::*, SeekPrecision::*};
        let cmd = mpv::NamedCommand::Seek {
            target: offset.into(),
            flags: Some((Relative, Exact).into()),
        };
        Ok(self.mpv.command(cmd).sync().await?)
    }

    async fn set_position(&self, track_id: TrackId, position: Time) -> fdo::Result<()> {
        let id = track_id
            .strip_prefix("/io/mpv/playlist_entry_id/")
            .map(str::parse)
            .transpose()
            .map_err(|e| fdo::Error::InvalidArgs(format!("Invalid track ID: {e}")))?;
        if id.is_some() && id == self.state.read().await.playlist_entry_id {
            let value: mpv::Seconds = position.into();
            self.mpv.set_property("playback-time", value).sync().await?;
            return Ok(());
        }
        Err(fdo::Error::InvalidArgs("Invalid track ID".into()))
    }

    async fn open_uri(&self, uri: String) -> fdo::Result<()> {
        let cmd = mpv::NamedCommand::Loadfile {
            url: uri,
            flags: Some(mpv::LoadFlags::Replace),
            index: None,
            options: None,
        };
        Ok(self.mpv.command(cmd).sync().await?)
    }

    async fn playback_status(&self) -> fdo::Result<PlaybackStatus> {
        Ok(self.state.read().await.playback_status())
    }

    async fn loop_status(&self) -> fdo::Result<LoopStatus> {
        Ok(self.state.read().await.loop_status())
    }

    async fn set_loop_status(&self, loop_status: LoopStatus) -> zbus::Result<()> {
        self.mpv
            .set_property(
                "loop-file",
                match loop_status {
                    LoopStatus::Track => "inf",
                    _ => "no",
                },
            )
            .sync()
            .await?;
        self.mpv
            .set_property(
                "loop-playlist",
                match loop_status {
                    LoopStatus::Playlist => "inf",
                    _ => "no",
                },
            )
            .sync()
            .await?;
        Ok(())
    }

    async fn rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(self.state.read().await.speed)
    }

    async fn set_rate(&self, rate: PlaybackRate) -> zbus::Result<()> {
        // The spec says a rate of 0.0 should act like Pause.
        if rate == 0.0 {
            return Ok(self.mpv.set_property("pause", true).sync().await?);
        }
        Ok(self.mpv.set_property("speed", rate).sync().await?)
    }

    async fn shuffle(&self) -> fdo::Result<bool> {
        Ok(self.state.read().await.shuffle)
    }

    async fn set_shuffle(&self, shuffle: bool) -> zbus::Result<()> {
        // Don't reshuffle if already shuffled.
        if self.state.read().await.shuffle == shuffle {
            return Ok(());
        }
        // Changing mpv's `shuffle` option doesn't reorder the current playlist,
        // so do that explicitly. The option is still set so `Shuffle` follows.
        let cmd = if shuffle {
            mpv::NamedCommand::PlaylistShuffle
        } else {
            mpv::NamedCommand::PlaylistUnshuffle
        };
        self.mpv.command(cmd).sync().await?;
        Ok(self.mpv.set_property("shuffle", shuffle).sync().await?)
    }

    async fn metadata(&self) -> fdo::Result<Metadata> {
        Ok(self.state.read().await.metadata())
    }

    async fn volume(&self) -> fdo::Result<Volume> {
        Ok(self.state.read().await.volume())
    }

    async fn set_volume(&self, volume: Volume) -> zbus::Result<()> {
        // The spec says negative values should set the volume to 0.
        let volume = volume.max(0.0) * 100.0;
        Ok(self.mpv.set_property("volume", volume).sync().await?)
    }

    async fn position(&self) -> fdo::Result<Time> {
        let position = self.mpv.get_property::<mpv::Seconds>("playback-time");
        match position.sync().await {
            Ok(position) => Ok(position.into()),
            // Nothing is loaded.
            Err(e) if e.is_property_unavailable() => Ok(Time::ZERO),
            Err(e) => Err(e.into()),
        }
    }

    async fn minimum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(0.01)
    }

    async fn maximum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(100.0)
    }

    async fn can_go_next(&self) -> fdo::Result<bool> {
        Ok(self.state.read().await.playlist_has_next())
    }

    async fn can_go_previous(&self) -> fdo::Result<bool> {
        Ok(self.state.read().await.playlist_has_previous())
    }

    async fn can_play(&self) -> fdo::Result<bool> {
        Ok(self.state.read().await.playlist_entry_id.is_some())
    }

    async fn can_pause(&self) -> fdo::Result<bool> {
        Ok(self.state.read().await.playlist_entry_id.is_some())
    }

    async fn can_seek(&self) -> fdo::Result<bool> {
        Ok(self.state.read().await.seekable)
    }

    async fn can_control(&self) -> fdo::Result<bool> {
        Ok(true)
    }
}

impl super::state::State {
    pub(super) fn playlist_has_next(&self) -> bool {
        self.playlist_current_pos
            .and_then(|x| x.checked_add(1))
            .zip(self.playlist_count)
            .is_some_and(|(current, count)| current < count)
    }

    pub(super) fn playlist_has_previous(&self) -> bool {
        self.playlist_current_pos.is_some_and(|current| 0 < current)
    }

    pub(super) fn playback_status(&self) -> PlaybackStatus {
        if self.idle_active || self.eof_reached {
            PlaybackStatus::Stopped
        } else if self.pause {
            PlaybackStatus::Paused
        } else {
            PlaybackStatus::Playing
        }
    }

    pub(super) fn loop_status(&self) -> LoopStatus {
        if self.loop_file {
            LoopStatus::Track
        } else if self.loop_playlist {
            LoopStatus::Playlist
        } else {
            LoopStatus::None
        }
    }

    pub(super) fn metadata(&self) -> Metadata {
        let track_id = ObjectPath::from_string_unchecked({
            let Some(playlist_entry_id) = self.playlist_entry_id else {
                return MetadataBuilder::default()
                    .trackid(ObjectPath::from_string_unchecked(
                        "/org/mpris/MediaPlayer2/TrackList/NoTrack".into(),
                    ))
                    .build();
            };
            format!("/io/mpv/playlist_entry_id/{playlist_entry_id}")
        });
        let url = match (&self.path, &self.working_directory) {
            (Some(mpv::Path::Url(url)), _) => Some(url.clone()),
            (Some(mpv::Path::Path(path)), working_directory) => {
                let path = if let Some(working_directory) = working_directory {
                    Cow::Owned(working_directory.join(path))
                } else {
                    Cow::Borrowed(path.as_path())
                };
                Url::from_file_path(path).ok()
            }
            _ => None,
        };
        let mut metadata = MetadataBuilder::default()
            .trackid(track_id)
            .length(self.duration.into())
            .title(self.media_title.to_owned())
            .build();
        metadata.set_art_url(self.art_url.clone());
        metadata.set_url(url);
        for (k, v) in &self.metadata {
            use crate::mpv::MetadataKey::*;
            let integer = |s: &str| s.split_once('/').map(|(s, _)| s).unwrap_or(s).parse().ok();
            match (k, v) {
                (Album, v) => metadata.set_album(v.into()),
                (AlbumArtist, v) => metadata.set_album_artist([v].into()),
                (Artist, v) => metadata.set_artist([v].into()),
                (Bpm, v) => metadata.set_audio_bpm(integer(v)),
                (Comment, v) => metadata.set_comment([v].into()),
                (Composer, v) => metadata.set_composer([v].into()),
                (Disc, v) => metadata.set_disc_number(integer(v)),
                (Genre, v) => metadata.set_genre([v].into()),
                (Lyricist, v) => metadata.set_lyricist([v].into()),
                (Track, v) => metadata.set_track_number(integer(v)),
                (Other(k), v) if k.to_ascii_lowercase().starts_with("lyrics") => {
                    metadata.set_lyrics(v.into());
                }
                _ => (),
            }
        }
        metadata
    }

    pub(super) fn volume(&self) -> Volume {
        self.volume as Volume / 100.0
    }
}

#[derive(Default)]
struct InterfaceChanges {
    changed: BTreeMap<Property, zvariant::Value<'static>>,
}

impl InterfaceChanges {
    async fn emit(
        &self,
        connection: &zbus::Connection,
        interface: InterfaceName<'static>,
    ) -> zbus::Result<()> {
        if self.changed.is_empty() {
            return Ok(());
        }
        // Every property is sent with its new value, so none are invalidated.
        let invalidated: &[Property] = &[];
        connection
            .emit_signal(
                None::<zbus::names::BusName<'static>>,
                "/org/mpris/MediaPlayer2",
                fdo::Properties::name(),
                "PropertiesChanged",
                &(interface, &self.changed, invalidated),
            )
            .await
    }
}

#[derive(Default)]
pub(crate) struct PropertyChanges {
    root: InterfaceChanges,
    player: InterfaceChanges,
    art: Option<(PathBuf, u64)>,
}

impl PropertyChanges {
    pub(crate) async fn emit(&self, connection: &zbus::Connection) -> zbus::Result<()> {
        const ROOT: InterfaceName<'static> =
            InterfaceName::from_static_str_unchecked("org.mpris.MediaPlayer2");
        const PLAYER: InterfaceName<'static> =
            InterfaceName::from_static_str_unchecked("org.mpris.MediaPlayer2.Player");
        self.root.emit(connection, ROOT).await?;
        self.player.emit(connection, PLAYER).await?;
        Ok(())
    }

    pub(super) fn art(&mut self) -> Option<(PathBuf, u64)> {
        self.art.take()
    }

    fn change(
        &mut self,
        property: Property,
        value: zvariant::Value<'static>,
    ) -> Option<zvariant::Value<'static>> {
        if property.is_root() {
            &mut self.root.changed
        } else {
            &mut self.player.changed
        }
        .insert(property, value)
    }
}

impl super::Player {
    pub(crate) async fn update(&self, other: super::state::State) -> PropertyChanges {
        use Property as P;
        use StateDiff as S;

        let mut ret = PropertyChanges::default();
        let mut state = self.state.write().await;
        let other = mem::replace(&mut *state, other);

        if let (state_art, other_art) = (
            art::find(&state.track_list, &state.path, &state.working_directory),
            art::find(&other.track_list, &other.path, &other.working_directory),
        ) && state_art != other_art
        {
            match state_art {
                Some(art::Track::Embedded(path, index)) => {
                    ret.art = Some((path, index));
                }
                Some(art::Track::External(url)) => {
                    state.art_url = url.into();
                }
                _ => (),
            }
        }

        let state = RwLockWriteGuard::downgrade(state);
        let diff = state.diff(&other);
        if diff.contains(S::Fullscreen) {
            ret.change(P::Fullscreen, state.fullscreen.into());
        }
        if diff.contains(S::PlaylistEntryId) {
            ret.change(P::CanPlay, state.playlist_entry_id.is_some().into());
            ret.change(P::CanPause, state.playlist_entry_id.is_some().into());
        }
        if diff.contains(S::Seekable) {
            ret.change(P::CanSeek, state.seekable.into());
        }
        if diff.intersects(S::PlaylistCurrentPos | S::PlaylistCount) {
            ret.change(P::CanGoNext, state.playlist_has_next().into());
            ret.change(P::CanGoPrevious, state.playlist_has_previous().into());
        }
        if diff.intersects(S::IdleActive | S::EofReached | S::Pause) {
            ret.change(P::PlaybackStatus, state.playback_status().into());
        }
        if diff.intersects(S::LoopFile | S::LoopPlaylist) {
            ret.change(P::LoopStatus, state.loop_status().into());
        }
        if diff.contains(S::Speed) {
            ret.change(P::Rate, state.speed.into());
        }
        if diff.contains(S::Shuffle) {
            ret.change(P::Shuffle, state.shuffle.into());
        }
        if diff.contains(S::Volume) {
            ret.change(P::Volume, state.volume().into());
        }
        if diff.intersects(
            S::PlaylistEntryId
                | S::Duration
                | S::MediaTitle
                | S::Metadata
                | S::ArtUrl
                | S::Path
                | S::WorkingDirectory,
        ) {
            // The spec requires the new value, and clients that cache
            // properties (e.g. `playerctl --follow`) don't refetch invalidated ones.
            ret.change(P::Metadata, state.metadata().into());
        }
        ret
    }
}

#[derive(Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, zvariant::Type)]
#[zvariant(signature = "s")]
enum Property {
    CanQuit,
    Fullscreen,
    CanSetFullscreen,
    CanRaise,
    HasTrackList,
    Identity,
    DesktopEntry,
    SupportedUriSchemes,
    SupportedMimeTypes,
    PlaybackStatus,
    LoopStatus,
    Rate,
    Shuffle,
    Metadata,
    Volume,
    MinimumRate,
    MaximumRate,
    CanGoNext,
    CanGoPrevious,
    CanPlay,
    CanPause,
    CanSeek,
}

impl Property {
    const fn is_root(&self) -> bool {
        use Property::*;
        matches!(
            self,
            CanQuit
                | Fullscreen
                | CanSetFullscreen
                | CanRaise
                | HasTrackList
                | Identity
                | DesktopEntry
                | SupportedUriSchemes
                | SupportedMimeTypes
        )
    }
}

impl From<Time> for mpv::Seconds {
    fn from(time: Time) -> Self {
        Self::from(time.as_micros() as f64 / 1_000_000.0)
    }
}

impl From<mpv::Seconds> for Time {
    fn from(time: mpv::Seconds) -> Self {
        Self::from_micros((f64::from(time) * 1_000_000.0) as i64)
    }
}
