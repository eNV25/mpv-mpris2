use crate::{
    common::{FutureSyncExt, time_as_secs, time_from_secs},
    mpv::{ListCommand, LoadFlags, NamedCommand, Path, SeekFlags, SeekMode},
};
use mpris_server::{
    LoopStatus, Metadata, PlaybackRate, PlaybackStatus, PlayerInterface, RootInterface, Time,
    TrackId, Volume, builder::MetadataBuilder,
};
use serde::{Deserialize, Serialize};
use smol::lock::{OnceCell, RwLockWriteGuard};
use std::{borrow::Cow, collections::BTreeMap, mem};
use url::Url;
use zbus::{fdo, names::InterfaceName, object_server::Interface, zvariant, zvariant::ObjectPath};

impl RootInterface for super::Player {
    async fn raise(&self) -> fdo::Result<()> {
        Ok(())
    }

    async fn quit(&self) -> fdo::Result<()> {
        let cmd = NamedCommand::Quit { code: None };
        Ok(self.mpv.run_command(cmd).sync().await?)
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
        Ok("mpv".into())
    }

    async fn desktop_entry(&self) -> fdo::Result<String> {
        Ok("mpv Media Player".into())
    }

    async fn supported_uri_schemes(&self) -> fdo::Result<Vec<String>> {
        static SUPPORTED_URI_SCHEMES: OnceCell<Vec<String>> = OnceCell::new();
        Ok(SUPPORTED_URI_SCHEMES
            .get_or_init(|| async {
                self.mpv
                    .get_property::<String>("protocol-list")
                    .sync()
                    .await
                    .ok()
                    .iter()
                    .flat_map(|s| s.split(','))
                    .map(str::to_owned)
                    .collect()
            })
            .await
            .clone())
    }

    async fn supported_mime_types(&self) -> fdo::Result<Vec<String>> {
        #[crabtime::function]
        fn define_mpv_mime_types(pattern!($name:ident): _) {
            let value = format!(
                "{:?}",
                std::env::var("XDG_DATA_DIRS")
                    .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_owned())
                    .split(':')
                    .filter_map(|dir| {
                        let path = std::path::Path::new(dir).join("applications/mpv.desktop");
                        std::fs::read_to_string(path).ok()
                    })
                    .find_map(|content| {
                        content.lines().find_map(|line| {
                            line.strip_prefix("MimeType=").map(|v| {
                                v.split_terminator(';')
                                    .map(str::to_owned)
                                    .collect::<Vec<_>>()
                            })
                        })
                    })
                    .expect(
                        "Failed to find mpv.desktop at build time. Ensure mpv is installed, or set XDG_DATA_DIRS appropriately."
                    )
            );
            _ = value;
            crabtime::output! {
                const $name: &[&str] = &{{value}};
            }
        }
        define_mpv_mime_types!(MPV_MIME_TYPES);
        Ok(MPV_MIME_TYPES.iter().map(|&x| x.to_owned()).collect())
    }
}

impl PlayerInterface for super::Player {
    async fn next(&self) -> fdo::Result<()> {
        let cmd = NamedCommand::PlaylistNext { flags: None };
        Ok(self.mpv.run_command(cmd).sync().await?)
    }

    async fn previous(&self) -> fdo::Result<()> {
        let cmd = NamedCommand::PlaylistPrev { flags: None };
        Ok(self.mpv.run_command(cmd).sync().await?)
    }

    async fn pause(&self) -> fdo::Result<()> {
        Ok(self.mpv.set_property("pause", true).sync().await?)
    }

    async fn play_pause(&self) -> fdo::Result<()> {
        let cmd = ListCommand::Cycle("pause", None);
        Ok(self.mpv.run_command(cmd).sync().await?)
    }

    async fn stop(&self) -> fdo::Result<()> {
        self.pause().await?;
        let cmd = NamedCommand::Seek {
            target: 0.0,
            flags: Some(SeekFlags(Some(SeekMode::Absolute), None)),
        };
        Ok(self.mpv.run_command(cmd).sync().await?)
    }

    async fn play(&self) -> fdo::Result<()> {
        Ok(self.mpv.set_property("pause", false).sync().await?)
    }

    async fn seek(&self, offset: Time) -> fdo::Result<()> {
        let offset = time_as_secs(offset);
        let cmd = NamedCommand::Seek {
            target: offset,
            flags: Some(SeekFlags(Some(SeekMode::Relative), None)),
        };
        Ok(self.mpv.run_command(cmd).sync().await?)
    }

    async fn set_position(&self, track_id: TrackId, position: Time) -> fdo::Result<()> {
        let id = track_id
            .strip_prefix("/io/mpv/playlist_entry_id/")
            .map(str::parse)
            .transpose()
            .map_err(|e| fdo::Error::InvalidArgs(format!("Invalid track ID: {e}")))?;
        if id.is_some() && id == self.state.read().await.playlist_entry_id {
            let value = time_as_secs(position);
            self.mpv.set_property("playback-time", value).sync().await?;
            return Ok(());
        }
        Err(fdo::Error::InvalidArgs("Invalid track ID".into()))
    }

    async fn open_uri(&self, uri: String) -> fdo::Result<()> {
        let cmd = NamedCommand::Loadfile {
            url: uri,
            flags: Some(LoadFlags::Replace),
            index: None,
            options: None,
        };
        Ok(self.mpv.run_command(cmd).sync().await?)
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
        Ok(self.mpv.set_property("speed", rate).sync().await?)
    }

    async fn shuffle(&self) -> fdo::Result<bool> {
        Ok(self.state.read().await.shuffle)
    }

    async fn set_shuffle(&self, shuffle: bool) -> zbus::Result<()> {
        Ok(self.mpv.set_property("shuffle", shuffle).sync().await?)
    }

    async fn metadata(&self) -> fdo::Result<Metadata> {
        let state = self.state.read().await;
        state.metadata().map_err(fdo::Error::Failed)
    }

    async fn volume(&self) -> fdo::Result<Volume> {
        Ok(self.state.read().await.volume())
    }

    async fn set_volume(&self, volume: Volume) -> zbus::Result<()> {
        let volume = volume * 100.0;
        Ok(self.mpv.set_property("volume", volume).sync().await?)
    }

    async fn position(&self) -> fdo::Result<Time> {
        let position = self.mpv.get_property("playback-time").sync().await?;
        Ok(time_from_secs(position))
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

    pub(super) fn metadata(&self) -> Result<Metadata, String> {
        let track_id = ObjectPath::from_string_unchecked({
            let Some(playlist_entry_id) = self.playlist_entry_id else {
                return Err("No track".into());
            };
            format!("/io/mpv/playlist_entry_id/{playlist_entry_id}")
        });
        let url = match (&self.path, &self.working_directory) {
            (Some(Path::Url(url)), _) => Some(url.clone()),
            (Some(Path::Path(path)), working_directory) => {
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
            .length(time_from_secs(self.duration))
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
        Ok(metadata)
    }

    pub(super) fn volume(&self) -> Volume {
        self.volume as Volume / 100.0
    }
}

#[derive(Default)]
struct InterfaceChanges {
    changed: BTreeMap<Property, zvariant::Value<'static>>,
    invalid: Vec<Property>,
}

impl InterfaceChanges {
    fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.invalid.is_empty()
    }
    async fn emit(
        &self,
        connection: &zbus::Connection,
        interface: InterfaceName<'static>,
    ) -> zbus::Result<()> {
        if self.is_empty() {
            return Ok(());
        }
        connection
            .emit_signal(
                None::<zbus::names::BusName<'static>>,
                "/org/mpris/MediaPlayer2",
                fdo::Properties::name(),
                "PropertiesChanged",
                &(interface, &self.changed, self.invalid.as_slice()),
            )
            .await
    }
}

#[derive(Default)]
pub(crate) struct PropertyChanges {
    root: InterfaceChanges,
    player: InterfaceChanges,
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

    fn invalidate(&mut self, property: Property) {
        if property.is_root() {
            &mut self.root.invalid
        } else {
            &mut self.player.invalid
        }
        .push(property);
    }
}

impl super::Player {
    pub(crate) async fn update(&self, other: &mut super::state::State) -> PropertyChanges {
        use Property::*;

        let mut state = self.state.write().await;
        mem::swap(&mut *state, other);

        if let (state_art, other_art) = (
            super::art_info(&state.track_list, &state.path, &state.working_directory),
            super::art_info(&other.track_list, &other.path, &other.working_directory),
        ) && state_art != other_art
        {
            match state_art {
                Some(super::ArtInfo::Embedded(path, index)) => {
                    other.art_index = Some((path, index));
                }
                Some(super::ArtInfo::External(url)) => {
                    state.art_url = url.into();
                }
                _ => (),
            }
        }

        let mut ret = PropertyChanges::default();
        let state = RwLockWriteGuard::downgrade(state);
        if state.fullscreen != other.fullscreen {
            ret.change(Fullscreen, state.fullscreen.into());
        }
        if state.playlist_entry_id.is_some() != other.playlist_entry_id.is_some() {
            ret.change(CanPlay, state.playlist_entry_id.is_some().into());
            ret.change(CanPause, state.playlist_entry_id.is_some().into());
        }
        if state.seekable != other.seekable {
            ret.change(CanSeek, state.seekable.into());
        }
        if state.playlist_current_pos != other.playlist_current_pos
            || state.playlist_count != other.playlist_count
        {
            if state.playlist_has_next() != other.playlist_has_next() {
                ret.change(CanGoNext, state.playlist_has_next().into());
            }
            if state.playlist_has_previous() != other.playlist_has_previous() {
                ret.change(CanGoPrevious, state.playlist_has_previous().into());
            }
        }
        if state.idle_active != other.idle_active
            || state.eof_reached != other.eof_reached
            || state.pause != other.pause
        {
            ret.change(PlaybackStatus, state.playback_status().into());
        }
        if state.loop_file != other.loop_file || state.loop_playlist != other.loop_playlist {
            ret.change(LoopStatus, state.loop_status().into());
        }
        if state.speed != other.speed {
            ret.change(Rate, state.speed.into());
        }
        if state.shuffle != other.shuffle {
            ret.change(Shuffle, state.shuffle.into());
        }
        if state.volume != other.volume {
            ret.change(Volume, state.volume().into());
        }
        if state.playlist_entry_id != other.playlist_entry_id
            || state.duration != other.duration
            || state.media_title != other.media_title
            || state.metadata != other.metadata
            || state.art_url != other.art_url
            || state.path != other.path
            || state.working_directory != other.working_directory
        {
            ret.invalidate(Metadata);
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
