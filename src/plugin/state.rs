use crate::mpv::{self, KnownProperty, LoopData, LoopVariant, MetadataKey, Mpv, Path, Track};
use smol::lock::RwLock;
use std::{collections::BTreeMap, path::PathBuf};
use url::Url;

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
    pub(crate) path: Option<Path>,
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
}
