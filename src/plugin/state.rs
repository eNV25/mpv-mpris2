use crate::{
    common::time_from_secs,
    mpv::{self, KnownProperty, LoopData, LoopVariant, MetadataKey, Mpv, Track},
};
use mpris_server::{LoopStatus, Metadata, PlaybackStatus, Volume, builder::MetadataBuilder};
use serde::{Deserialize, Serialize};
use smol::lock::{RwLock, RwLockWriteGuard};
use std::{
    collections::{BTreeMap, HashMap},
    mem,
    path::PathBuf,
};
use url::Url;
use zbus::{fdo, names::InterfaceName, object_server::Interface, zvariant, zvariant::ObjectPath};

#[derive(Clone, Debug, Default)]
pub(crate) struct State {
    pub(crate) fullscreen: bool,
    pub(crate) playlist_entry_id: Option<i64>,
    pub(crate) playlist_current_pos: Option<u64>,
    pub(crate) playlist_count: Option<u64>,
    pub(crate) seekable: bool,
    pub(crate) idle_active: bool,
    pub(crate) eof_reached: bool,
    pub(crate) pause: bool,
    pub(crate) loop_file: bool,
    pub(crate) loop_playlist: bool,
    pub(crate) speed: f64,
    pub(crate) shuffle: bool,
    pub(crate) volume: f64,
    pub(crate) duration: f64,
    pub(crate) media_title: String,
    pub(crate) metadata: BTreeMap<MetadataKey, String>,
    pub(crate) track_list: Vec<Track>,
    pub(crate) path: Option<PathBuf>,
    pub(crate) working_directory: Option<PathBuf>,
    pub(crate) art_url: Option<Url>,
    pub(crate) art_index: Option<(PathBuf, u64)>,
}

impl super::Player {
    pub(crate) async fn new(mpv: Mpv) -> Result<Self, mpv::Error> {
        async fn property<T: Default>(mpv: &Mpv, name: &'static str) -> Result<T, mpv::Error> {
            mpv.observe_property(name).await?;
            Ok(Default::default())
        }

        let state = RwLock::new(State {
            fullscreen: property(&mpv, "fullscreen").await?,
            playlist_entry_id: None,
            playlist_current_pos: property(&mpv, "playlist-current-pos").await?,
            playlist_count: property(&mpv, "playlist-count").await?,
            seekable: property(&mpv, "seekable").await?,
            idle_active: property(&mpv, "idle-active").await?,
            eof_reached: property(&mpv, "eof-reached").await?,
            pause: property(&mpv, "pause").await?,
            loop_file: property(&mpv, "loop-file").await?,
            loop_playlist: property(&mpv, "loop-playlist").await?,
            speed: property(&mpv, "speed").await?,
            shuffle: property(&mpv, "shuffle").await?,
            volume: property(&mpv, "volume").await?,
            duration: property(&mpv, "duration").await?,
            media_title: property(&mpv, "media-title").await?,
            metadata: property(&mpv, "metadata").await?,
            track_list: property(&mpv, "track-list").await?,
            path: property(&mpv, "path").await?,
            working_directory: property(&mpv, "working-directory").await?,
            art_url: None,
            art_index: None,
        });
        Ok(Self { mpv, state })
    }

    pub(crate) async fn state(&self) -> State {
        self.state.read().await.clone()
    }

    pub(crate) async fn update(&self, other: &mut State) -> PropertyChanges {
        use Property::*;

        let mut state = self.state.write().await;
        mem::swap(&mut *state, other);

        if let (state_art, other_art) = (
            art_info(&state.track_list, &state.path, &state.working_directory),
            art_info(&other.track_list, &other.path, &other.working_directory),
        ) && state_art != other_art
        {
            match state_art {
                Some(ArtInfo::Embedded(path, index)) => {
                    other.art_index = Some((path, index));
                }
                Some(ArtInfo::External(url)) => {
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

impl State {
    pub(crate) fn change(&mut self, property: KnownProperty) {
        fn loop_bool(loop_data: Option<LoopData>) -> bool {
            match loop_data {
                Some(LoopData::Bool(b)) => b,
                Some(LoopData::Number(n)) => n != 0,
                Some(LoopData::Variant(LoopVariant::Inf)) => true,
                Some(LoopData::Variant(LoopVariant::No)) => false,
                None => false,
            }
        }
        match property {
            KnownProperty::Fullscreen(fullscreen) => {
                self.fullscreen = fullscreen.unwrap_or_default();
            }
            KnownProperty::PlaylistCurrentPos(playlist_current_pos) => {
                self.playlist_current_pos = playlist_current_pos;
            }
            KnownProperty::PlaylistCount(playlist_count) => {
                self.playlist_count = playlist_count;
            }
            KnownProperty::Seekable(seekable) => {
                self.seekable = seekable.unwrap_or_default();
            }
            KnownProperty::IdleActive(idle_active) => {
                self.idle_active = idle_active.unwrap_or_default();
            }
            KnownProperty::EofReached(eof_reached) => {
                self.eof_reached = eof_reached.unwrap_or_default();
            }
            KnownProperty::Pause(pause) => {
                self.pause = pause.unwrap_or_default();
            }
            KnownProperty::LoopFile(loop_file) => {
                self.loop_file = loop_bool(loop_file);
            }
            KnownProperty::LoopPlaylist(loop_playlist) => {
                self.loop_playlist = loop_bool(loop_playlist);
            }
            KnownProperty::Speed(speed) => {
                self.speed = speed.unwrap_or_default();
            }
            KnownProperty::Shuffle(shuffle) => {
                self.shuffle = shuffle.unwrap_or_default();
            }
            KnownProperty::Volume(volume) => {
                self.volume = volume.unwrap_or_default();
            }
            KnownProperty::Duration(duration) => {
                self.duration = duration.unwrap_or_default();
            }
            KnownProperty::MediaTitle(media_title) => {
                self.media_title = media_title.unwrap_or_default();
            }
            KnownProperty::Metadata(metadata) => {
                self.metadata = metadata;
            }
            KnownProperty::TrackList(track_list) => {
                self.track_list = track_list;
            }
            KnownProperty::Path(path) => {
                self.path = path;
            }
            KnownProperty::WorkingDirectory(working_directory) => {
                self.working_directory = working_directory;
            }
        }
    }

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
            (Some(path), Some(working_directory)) => {
                Url::from_file_path(working_directory.join(path)).ok()
            }
            (Some(path), None) => Url::from_file_path(path).ok(),
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
    changed: HashMap<Property, zvariant::Value<'static>>,
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

#[derive(Debug, PartialEq, Eq, Hash, Serialize, Deserialize, zvariant::Type)]
#[zvariant(signature = "s")]
pub(crate) enum Property {
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

fn art_info(
    track_list: &[Track],
    path: &Option<PathBuf>,
    working_directory: &Option<PathBuf>,
) -> Option<ArtInfo> {
    let path = path.as_ref();
    let working_directory = working_directory.as_ref();
    let mut art_index = None;
    let mut art_filename = None;
    let track_list_len = track_list.len();
    for track in track_list {
        match track {
            Track::ExternalAlbumArt {
                external_filename, ..
            } => {
                art_filename = working_directory.map(|w| w.join(external_filename));
            }
            Track::ExternalImage {
                external_filename, ..
            } => {
                art_filename =
                    art_filename.or_else(|| working_directory.map(|w| w.join(external_filename)));
            }
            &Track::EmbeddedAlbumArt { ff_index, .. } => {
                art_index = Some(ff_index);
            }
            &Track::EmbeddedImage { ff_index, .. } => {
                if track_list_len == 1 {
                    art_filename = working_directory.zip(path).map(|(w, p)| w.join(p));
                } else {
                    art_index.get_or_insert(ff_index);
                }
            }
            Track::None(_) => (),
        }
    }
    let art_filename = || {
        art_filename
            .and_then(|path| Url::from_file_path(path).ok())
            .map(ArtInfo::External)
    };
    let art_index = || {
        art_index
            .zip(path)
            .map(|(index, path)| ArtInfo::Embedded(path.clone(), index))
    };
    art_filename().or_else(art_index)
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ArtInfo {
    Embedded(PathBuf, u64),
    External(Url),
}
