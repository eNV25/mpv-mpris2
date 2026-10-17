use crate::mpv;
use compact_str::format_compact;
use std::{path::PathBuf, process::Stdio};
use tempfile::NamedTempFile;
use url::Url;
use zruntime::{LocalRuntime, process::Command};

pub(super) struct State {
    task: Option<zruntime::Task<()>>,
    file: Option<NamedTempFile>,
    tx: kanal::AsyncSender<NamedTempFile>,
}

impl State {
    pub(super) fn new() -> (Self, kanal::AsyncReceiver<NamedTempFile>) {
        let (tx, rx) = kanal::bounded_async(0);
        let this = Self {
            task: None,
            file: None,
            tx,
        };
        (this, rx)
    }

    /// Stops any extraction in progress, and deletes the current art file.
    pub(super) fn clear(&mut self) {
        self.task = None;
        self.file = None;
    }

    /// Extracts the image in stream `index` of `path`, replacing any extraction
    /// in progress. The file is sent to the receiver returned by [`State::new`].
    pub(super) fn spawn_worker(&mut self, runtime: &LocalRuntime, path: PathBuf, index: u64) {
        let tx = self.tx.clone();
        self.task = Some(zruntime::spawn_local({
            let runtime = runtime.clone();
            async move {
                if let Err(e) = worker(&runtime, tx, path, index).await {
                    tracing::error!(error = %e, "Failed to extract embedded art");
                }
            }
        }));
    }

    pub(super) fn set_file(&mut self, file: NamedTempFile) {
        self.file = Some(file);
    }
}

async fn worker(
    runtime: &LocalRuntime,
    tx: kanal::AsyncSender<NamedTempFile>,
    path: PathBuf,
    index: u64,
) -> anyhow::Result<()> {
    let file = tempfile::Builder::new()
        .prefix("mpv-mpris2-art-")
        .suffix(".jpg")
        .tempfile()?;
    let status = Command::new("ffmpeg")
        .arg("-i")
        .arg(&path)
        .arg("-map")
        .arg(format_compact!("0:{index}"))
        .args(["-c:v", "mjpeg", "-q:v", "2", "-f", "image2pipe", "-"])
        .stdin(Stdio::null())
        .stdout(file.reopen()?)
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn(runtime)?
        .status()
        .await?;
    anyhow::ensure!(status.success(), "ffmpeg failed: {status}");
    anyhow::ensure!(
        file.as_file().metadata()?.len() > 0,
        "ffmpeg wrote no image"
    );
    if let Err(e) = tx.send(file).await {
        tracing::error!(error = %e, "Failed to send art url");
    }
    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum Track {
    Embedded(PathBuf, u64),
    External(Url),
}

/// Finds the current file's cover art. External art beats embedded art, and
/// album art beats other images. An image played on its own is its own art.
pub(super) fn find(
    track_list: &[mpv::Track],
    path: &Option<mpv::Path>,
    working_directory: &Option<PathBuf>,
) -> Option<Track> {
    let path = path.as_ref().and_then(|x| match x {
        mpv::Path::Path(path) => Some(path),
        _ => None,
    });
    let mut art_index = None;
    let mut art_filename = None;
    for track in track_list {
        match track {
            mpv::Track::ExternalAlbumArt {
                external_filename, ..
            } => {
                art_filename = Some(external_filename);
            }
            mpv::Track::ExternalImage {
                external_filename, ..
            } => {
                art_filename.get_or_insert(external_filename);
            }
            &mpv::Track::EmbeddedAlbumArt { ff_index, .. } => {
                art_index = Some(ff_index);
            }
            &mpv::Track::EmbeddedImage { ff_index, .. } => {
                if track_list.len() == 1 {
                    art_filename = path;
                } else {
                    art_index.get_or_insert(ff_index);
                }
            }
            mpv::Track::None(_) => (),
        }
    }
    if let Some(file) = art_filename {
        let path = working_directory.as_ref().map(|dir| dir.join(file));
        let path = path.unwrap_or_else(|| file.clone());
        return Some(Track::External(Url::from_file_path(path).ok()?));
    }
    if let Some(index) = art_index
        && let Some(path) = path
    {
        return Some(Track::Embedded(path.clone(), index));
    }
    None
}
