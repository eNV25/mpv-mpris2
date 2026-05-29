#![allow(unused)]

use compact_str::CompactString;
use serde::{Deserialize, Serialize, Serializer, de::IgnoredAny, ser, ser::SerializeSeq};
use serde_constant::ConstBool;
use serde_json::Value;
use serde_variant::to_variant_name;
use serde_with::DeserializeFromStr;
use std::{collections::BTreeMap, fmt::Debug, path::PathBuf};
use strum::{EnumDiscriminants, EnumString};

#[derive(Serialize)]
pub(super) struct Request {
    pub(super) command: Command,
    pub(super) request_id: i64,
    pub(super) r#async: ConstBool<true>,
}

#[derive(Serialize)]
#[serde(untagged)]
pub(crate) enum Command {
    Raw(Vec<Value>),
    List(ListCommand),
    Named(NamedCommand),
}

impl From<Vec<Value>> for Command {
    fn from(value: Vec<Value>) -> Self {
        Command::Raw(value)
    }
}

impl From<ListCommand> for Command {
    fn from(value: ListCommand) -> Self {
        Command::List(value)
    }
}

impl From<NamedCommand> for Command {
    fn from(value: NamedCommand) -> Self {
        Command::Named(value)
    }
}

#[derive(EnumDiscriminants)]
#[strum_discriminants(derive(Serialize), serde(rename_all = "snake_case"))]
pub(crate) enum ListCommand {
    ClientName,
    GetTimeUs,
    GetProperty(&'static str),
    GetPropertyString(&'static str),
    SetProperty(&'static str, Value),
    SetPropertyString(&'static str, CompactString),
    ObserveProperty(i64, &'static str),
    ObservePropertyString(i64, &'static str),
    UnobserveProperty(i64),
    GetVersion,
    Cycle(&'static str, Option<CycleDirection>),
}

#[derive(Serialize)]
pub(crate) enum CycleDirection {
    Up,
    Down,
}

impl Serialize for ListCommand {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use ListCommandDiscriminants::*;
        let mut seq = serializer.serialize_seq(None)?;
        match self {
            ListCommand::ClientName => {
                seq.serialize_element(&ClientName)?;
            }
            ListCommand::GetTimeUs => {
                seq.serialize_element(&GetTimeUs)?;
            }
            ListCommand::GetProperty(x) => {
                seq.serialize_element(&GetProperty)?;
                seq.serialize_element(x)?;
            }
            ListCommand::GetPropertyString(x) => {
                seq.serialize_element(&GetPropertyString)?;
                seq.serialize_element(x)?;
            }
            ListCommand::SetProperty(x, y) => {
                seq.serialize_element(&SetProperty)?;
                seq.serialize_element(x)?;
                seq.serialize_element(y)?;
            }
            ListCommand::SetPropertyString(x, y) => {
                seq.serialize_element(&SetPropertyString)?;
                seq.serialize_element(x)?;
                seq.serialize_element(y)?;
            }
            ListCommand::ObserveProperty(x, y) => {
                seq.serialize_element(&ObserveProperty)?;
                seq.serialize_element(x)?;
                seq.serialize_element(y)?;
            }
            ListCommand::ObservePropertyString(x, y) => {
                seq.serialize_element(&ObservePropertyString)?;
                seq.serialize_element(x)?;
                seq.serialize_element(y)?;
            }
            ListCommand::UnobserveProperty(x) => {
                seq.serialize_element(&UnobserveProperty)?;
                seq.serialize_element(x)?;
            }
            ListCommand::GetVersion => {
                seq.serialize_element(&GetVersion)?;
            }
            ListCommand::Cycle(x, y) => {
                seq.serialize_element(&Cycle)?;
                seq.serialize_element(x)?;
                if let Some(y) = y {
                    seq.serialize_element(y)?;
                }
            }
        }
        seq.end()
    }
}

#[derive(Serialize)]
#[serde(tag = "name", rename_all = "kebab-case")]
pub(crate) enum NamedCommand {
    Seek {
        target: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        flags: Option<SeekFlags>,
    },
    Loadfile {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        flags: Option<LoadFlags>,
        #[serde(skip_serializing_if = "Option::is_none")]
        index: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        options: Option<BTreeMap<CompactString, CompactString>>,
    },
    Loadlist {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        flags: Option<LoadFlags>,
        #[serde(skip_serializing_if = "Option::is_none")]
        index: Option<i64>,
    },
    PlaylistNext {
        #[serde(skip_serializing_if = "Option::is_none")]
        flags: Option<PlaylistFlags>,
    },
    PlaylistPrev {
        #[serde(skip_serializing_if = "Option::is_none")]
        flags: Option<PlaylistFlags>,
    },
    Quit {
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<i64>,
    },
}

pub(crate) struct SeekFlags(
    pub(crate) Option<SeekMode>,
    pub(crate) Option<SeekPrecision>,
);

#[derive(Serialize, Copy, Clone)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SeekMode {
    Relative,
    Absolute,
    AbsolutePercent,
    RelativePercent,
}

#[derive(Serialize, Copy, Clone)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SeekPrecision {
    Keyframes,
    Exact,
}

impl Serialize for SeekFlags {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match (self.0, self.1) {
            (Some(mode), None) => {
                let mode = to_variant_name(&mode).map_err(ser::Error::custom)?;
                serializer.serialize_str(mode)
            }
            (None, Some(precision)) => {
                let precision = to_variant_name(&precision).map_err(ser::Error::custom)?;
                serializer.serialize_str(precision)
            }
            (Some(mode), Some(precision)) => {
                let mode = to_variant_name(&mode).map_err(ser::Error::custom)?;
                let precision = to_variant_name(&precision).map_err(ser::Error::custom)?;
                serializer.collect_str(&format_args!("{mode}+{precision}"))
            }
            (None, None) => serializer.serialize_unit(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum LoadFlags {
    Replace,
    Append,
    AppendPlay,
    InsertNext,
    InsertNextPlay,
    InsertAt,
    InsertAtPlay,
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PlaylistFlags {
    Weak,
    Force,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum Response {
    CommandResponseSuccess {
        #[serde(default)]
        data: Value,
        request_id: i64,
        error: CommandResponseSuccess,
    },
    CommandResponseFailure {
        request_id: i64,
        error: CompactString,
    },
    Event(Event),
    UnknownEvent {
        event: CompactString,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum CommandResponseSuccess {
    Success,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub(crate) enum Event {
    StartFile {
        playlist_entry_id: i64,
    },
    EndFile {
        reason: EndFileReason,
        playlist_entry_id: i64,
        #[serde(default)]
        file_error: Option<CompactString>,
        #[serde(default)]
        playlist_insert_id: Option<i64>,
        #[serde(default)]
        playlist_insert_num_entries: Option<i64>,
    },
    FileLoaded,
    Seek,
    PlaybackRestart,
    Shutdown,
    LogMessage {
        prefix: CompactString,
        level: CompactString,
        text: CompactString,
    },
    ClientMessage {
        args: Vec<CompactString>,
    },
    VideoReconfig,
    AudioReconfig,
    PropertyChange(Property),
    #[serde(skip_deserializing)]
    Seeked {
        playback_time: f64,
    },
    #[serde(skip_deserializing)]
    Unknown(CompactString),
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum EndFileReason {
    Eof,
    Stop,
    Quit,
    Error,
    Redirect,
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub(crate) enum Property {
    Known(KnownProperty),
    Unknown { name: CompactString, data: Value },
}

#[derive(Deserialize, Debug)]
#[serde(tag = "name", content = "data", rename_all = "kebab-case")]
pub(crate) enum KnownProperty {
    Fullscreen(#[serde(default)] Option<bool>),
    PlaylistCurrentPos(#[serde(default)] Option<u64>),
    PlaylistCount(#[serde(default)] Option<u64>),
    Seekable(#[serde(default)] Option<bool>),
    IdleActive(#[serde(default)] Option<bool>),
    EofReached(#[serde(default)] Option<bool>),
    Pause(#[serde(default)] Option<bool>),
    LoopFile(#[serde(default)] Option<LoopData>),
    LoopPlaylist(#[serde(default)] Option<LoopData>),
    Speed(#[serde(default)] Option<f64>),
    Shuffle(#[serde(default)] Option<bool>),
    Volume(#[serde(default)] Option<f64>),
    Duration(#[serde(default)] Option<f64>),
    MediaTitle(#[serde(default)] Option<String>),
    Metadata(#[serde(default)] BTreeMap<MetadataKey, String>),
    TrackList(#[serde(default)] Vec<Track>),
    Path(#[serde(default)] Option<PathBuf>),
    WorkingDirectory(#[serde(default)] Option<PathBuf>),
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub(crate) enum LoopData {
    Bool(bool),
    Number(u64),
    Variant(LoopVariant),
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum LoopVariant {
    Inf,
    No,
}

#[derive(DeserializeFromStr, EnumString, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[strum(serialize_all = "snake_case", ascii_case_insensitive)]
pub(crate) enum MetadataKey {
    Album,
    AlbumArtist,
    Artist,
    #[strum(serialize = "bpm", serialize = "tbp", serialize = "tbpm")]
    Bpm,
    Comment,
    Composer,
    Disc,
    Genre,
    Lyricist,
    Track,
    #[strum(default)]
    Other(CompactString),
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub(crate) enum Track {
    #[serde(rename_all = "kebab-case")]
    EmbeddedAlbumArt {
        image: ConstBool<true>,
        albumart: ConstBool<true>,
        external: ConstBool<false>,
        ff_index: u64,
    },
    #[serde(rename_all = "kebab-case")]
    EmbeddedImage {
        image: ConstBool<true>,
        albumart: ConstBool<false>,
        external: ConstBool<false>,
        ff_index: u64,
    },
    #[serde(rename_all = "kebab-case")]
    ExternalAlbumArt {
        image: ConstBool<true>,
        albumart: ConstBool<true>,
        external: ConstBool<true>,
        external_filename: PathBuf,
    },
    #[serde(rename_all = "kebab-case")]
    ExternalImage {
        image: ConstBool<true>,
        albumart: ConstBool<false>,
        external: ConstBool<true>,
        external_filename: PathBuf,
    },
    None(IgnoredAny),
}
