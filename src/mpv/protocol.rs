use compact_str::CompactString;
use derive_deftly::{Deftly, derive_deftly_adhoc};
use serde::{Deserialize, Serialize, Serializer, de::IgnoredAny, ser, ser::SerializeSeq};
use serde_constant::ConstBool;
use serde_json::Value;
use serde_variant::to_variant_name;
use serde_with::DeserializeFromStr;
use std::{collections::BTreeMap, fmt::Debug, path::PathBuf};
use strum::{EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};
use url::Url;

#[derive(Serialize)]
pub(super) struct Request {
    pub(super) command: Command,
    pub(super) request_id: i64,
    pub(super) r#async: ConstBool<true>,
}

#[derive(Serialize)]
#[serde(untagged)]
pub(crate) enum Command {
    List(ListCommand),
    Named(NamedCommand),
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

/// A command sent as a JSON array: its snake_case name, then its fields.
#[derive(Deftly)]
#[derive_deftly_adhoc]
pub(crate) enum ListCommand {
    GetProperty(&'static str),
    SetProperty(&'static str, Value),
    ObserveProperty(i64, &'static str),
    Cycle(&'static str),
}

derive_deftly_adhoc! {
    ListCommand:

    impl Serialize for ListCommand {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut seq = serializer.serialize_seq(None)?;
            match self {
                $($vpat => {
                    seq.serialize_element(stringify!(${snake_case $vname}))?;
                    $( seq.serialize_element($fpatname)?; )
                })
            }
            seq.end()
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "name", rename_all = "kebab-case")]
pub(crate) enum NamedCommand {
    Seek {
        target: Seconds,
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
    PlaylistNext,
    PlaylistPrev,
    PlaylistShuffle,
    PlaylistUnshuffle,
    Quit {
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<i64>,
    },
}

pub(crate) struct SeekFlags(
    pub(crate) Option<SeekMode>,
    pub(crate) Option<SeekPrecision>,
);

impl From<(SeekMode, SeekPrecision)> for SeekFlags {
    fn from(value: (SeekMode, SeekPrecision)) -> Self {
        let (mode, precision) = value;
        Self(mode.into(), precision.into())
    }
}

impl From<SeekMode> for SeekFlags {
    fn from(value: SeekMode) -> Self {
        Self(value.into(), None)
    }
}

impl From<SeekPrecision> for SeekFlags {
    fn from(value: SeekPrecision) -> Self {
        Self(None, value.into())
    }
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SeekMode {
    Relative,
    Absolute,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SeekPrecision {
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
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum Response {
    CommandResponseSuccess {
        #[serde(default)]
        data: Value,
        request_id: i64,
        #[allow(dead_code, reason = "only checks that the reply is a success")]
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

#[derive(Debug, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub(crate) enum Event {
    StartFile {
        playlist_entry_id: i64,
    },
    EndFile,
    FileLoaded,
    Seek,
    PlaybackRestart,
    Shutdown,
    VideoReconfig,
    AudioReconfig,
    PropertyChange(Property),
    #[serde(skip_deserializing)]
    Seeked {
        playback_time: Seconds,
    },
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum Property {
    Known(KnownProperty),
    /// A property that isn't known, or whose value didn't parse.
    Unknown {
        name: CompactString,
        data: Value,
    },
}

macro_rules! make_known_property {
    ($($name:ident($type:ty)),* $(,)?) => {
        #[derive(Debug, Deserialize, EnumDiscriminants)]
        #[serde(tag = "name", rename_all = "kebab-case")]
        #[strum_discriminants(
            name(KnownPropertyName),
            derive(EnumIter, IntoStaticStr),
            strum(serialize_all = "kebab-case")
        )]
        pub(crate) enum KnownProperty {
            $($name { #[serde(default)] data: Option<$type> }),*
        }
    };
}

make_known_property! {
    Fullscreen(bool),
    // -1 when there is no current entry, e.g. after `stop`.
    PlaylistCurrentPos(i64),
    PlaylistCount(u64),
    Seekable(bool),
    IdleActive(bool),
    EofReached(bool),
    Pause(bool),
    LoopFile(LoopData),
    LoopPlaylist(LoopData),
    Speed(f64),
    Shuffle(bool),
    Volume(f64),
    Duration(Seconds),
    MediaTitle(String),
    Metadata(BTreeMap<MetadataKey, String>),
    TrackList(Vec<Track>),
    Path(Path),
    WorkingDirectory(PathBuf),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum LoopData {
    Bool(bool),
    Number(u64),
    Variant(LoopVariant),
}

impl From<LoopData> for bool {
    fn from(value: LoopData) -> Self {
        match value {
            LoopData::Bool(b) => b,
            LoopData::Number(n) => n != 0,
            LoopData::Variant(LoopVariant::Inf | LoopVariant::Force) => true,
            LoopData::Variant(LoopVariant::No) => false,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum LoopVariant {
    Inf,
    No,
    // Only valid for `loop-playlist`.
    Force,
}

#[derive(Clone, Debug, DeserializeFromStr, EnumString, Eq, Ord, PartialEq, PartialOrd)]
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

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
#[allow(dead_code, reason = "the `ConstBool` fields only select the variant")]
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(untagged)]
pub(crate) enum Path {
    Url(Url),
    Path(PathBuf),
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(transparent)]
#[repr(transparent)]
pub(crate) struct Seconds(f64);

impl From<f64> for Seconds {
    fn from(time: f64) -> Self {
        Self(time)
    }
}

impl From<Seconds> for f64 {
    fn from(time: Seconds) -> Self {
        time.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    // Properties are observed by their `KnownPropertyName`, so it must match
    // the name serde parses. mpv leaves `data` out when there is no value.
    #[test]
    fn observed_properties_parse_without_data() {
        for name in KnownPropertyName::iter() {
            let line = serde_json::json!({
                "event": "property-change",
                "id": 0,
                "name": <&str>::from(name),
            })
            .to_string();
            let response: Response = serde_json::from_str(&line).unwrap();
            let Response::Event(Event::PropertyChange(Property::Known(property))) = response else {
                panic!("{line} didn't parse as a known property");
            };
            assert_eq!(KnownPropertyName::from(&property), name);
        }
    }

    // Unit variants must still match when mpv sends fields the plugin ignores.
    #[test]
    fn end_file_parses_with_fields() {
        let line = r#"{"event":"end-file","reason":"eof","playlist_entry_id":1}"#;
        let response: Response = serde_json::from_str(line).unwrap();
        assert!(matches!(response, Response::Event(Event::EndFile)));
    }
}
