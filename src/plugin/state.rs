use crate::mpv::{self, Mpv};
use derive_deftly::{Deftly, derive_deftly_adhoc};
use enumflags2::{BitFlags, bitflags};
use zruntime::lock::RwLock;
use std::{collections::BTreeMap, path::PathBuf};
use strum::IntoEnumIterator;
use url::Url;

#[derive(Clone, Debug, Default, Deftly)]
#[derive_deftly_adhoc]
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
    pub(crate) duration: mpv::Seconds,
    pub(crate) media_title: String,
    pub(crate) metadata: BTreeMap<mpv::MetadataKey, String>,
    pub(crate) path: Option<mpv::Path>,
    pub(crate) working_directory: Option<PathBuf>,
    pub(crate) art_url: Option<Url>,
    #[deftly(skip)]
    pub(crate) track_list: Vec<mpv::Track>,
}

impl super::Player {
    pub(crate) async fn new(mpv: Mpv) -> Result<Self, mpv::Error> {
        // Observe every property `State::change` handles. mpv sends each one's
        // current value as a property-change event, which `main_loop` applies.
        for name in mpv::KnownPropertyName::iter() {
            mpv.observe_property(name).await?;
        }
        let state = RwLock::new(State::default());
        Ok(Self { mpv, state })
    }

    pub(crate) async fn state(&self) -> State {
        self.state.read().await.clone()
    }
}

impl State {
    pub(crate) fn change(&mut self, property: mpv::KnownProperty) {
        use mpv::KnownProperty;
        match property {
            KnownProperty::Fullscreen { data } => {
                self.fullscreen = data.unwrap_or_default();
            }
            KnownProperty::PlaylistCurrentPos { data } => {
                // Negative means there is no current entry.
                self.playlist_current_pos = data.and_then(|pos| u64::try_from(pos).ok());
            }
            KnownProperty::PlaylistCount { data } => {
                self.playlist_count = data;
            }
            KnownProperty::Seekable { data } => {
                self.seekable = data.unwrap_or_default();
            }
            KnownProperty::IdleActive { data } => {
                self.idle_active = data.unwrap_or_default();
            }
            KnownProperty::EofReached { data } => {
                self.eof_reached = data.unwrap_or_default();
            }
            KnownProperty::Pause { data } => {
                self.pause = data.unwrap_or_default();
            }
            KnownProperty::LoopFile { data } => {
                self.loop_file = data.map(bool::from).unwrap_or_default();
            }
            KnownProperty::LoopPlaylist { data } => {
                self.loop_playlist = data.map(bool::from).unwrap_or_default();
            }
            KnownProperty::Speed { data } => {
                self.speed = data.unwrap_or_default();
            }
            KnownProperty::Shuffle { data } => {
                self.shuffle = data.unwrap_or_default();
            }
            KnownProperty::Volume { data } => {
                self.volume = data.unwrap_or_default();
            }
            KnownProperty::Duration { data } => {
                self.duration = data.unwrap_or_default();
            }
            KnownProperty::MediaTitle { data } => {
                self.media_title = data.unwrap_or_default();
            }
            KnownProperty::Metadata { data } => {
                self.metadata = data.unwrap_or_default();
            }
            KnownProperty::TrackList { data } => {
                self.track_list = data.unwrap_or_default();
            }
            KnownProperty::Path { data } => {
                self.path = data;
            }
            KnownProperty::WorkingDirectory { data } => {
                self.working_directory = data;
            }
        }
    }
}

derive_deftly_adhoc! {
    State:

    #[bitflags]
    #[repr(u32)]
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub(super) enum StateDiff {
        $(
            ${when not(fmeta(skip))}
            ${pascal_case $fname},
        )
    }

    impl State {
        pub(super) fn diff(&self, other: &Self) -> BitFlags<StateDiff> {
            let mut diff = BitFlags::empty();
            $(
                ${when not(fmeta(skip))}
                if self.$fname != other.$fname {
                    diff |= StateDiff::${pascal_case $fname};
                }
            )
            diff
        }
    }
}
