use crate::mpv;
use compact_str::format_compact;
use smol::process::Command;
use std::{path::PathBuf, process::Stdio};
use tempfile::NamedTempFile;
use url::Url;

pub(super) struct State {
    task: Option<smol::Task<anyhow::Result<()>>>,
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

    pub(super) fn clear(&mut self) {
        drop(self.task.take());
        drop(self.file.take());
    }

    pub(super) fn spawn_worker(&mut self, ex: &smol::LocalExecutor, path: PathBuf, index: u64) {
        self.task = Some(ex.spawn(worker(self.tx.clone(), path, index)));
    }

    pub(super) fn set_file(&mut self, file: NamedTempFile) {
        self.file = Some(file);
    }
}

async fn worker(
    tx: kanal::AsyncSender<NamedTempFile>,
    path: PathBuf,
    index: u64,
) -> anyhow::Result<()> {
    let file = tempfile::Builder::new()
        .prefix("mpv-mpris2-art-")
        .suffix(".jpg")
        .tempfile()?;
    _ = Command::new("ffmpeg")
        .arg("-i")
        .arg(&path)
        .arg("-map")
        .arg(format_compact!("0:{index}"))
        .args(["-c:v", "mjpeg", "-q:v", "2", "-f", "image2pipe", "-"])
        .stdin(Stdio::null())
        .stdout(file.reopen()?)
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?
        .status()
        .await?;
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
                _ = art_filename.insert(external_filename);
            }
            mpv::Track::ExternalImage {
                external_filename, ..
            } => {
                _ = art_filename.get_or_insert(external_filename);
            }
            &mpv::Track::EmbeddedAlbumArt { ff_index, .. } => {
                _ = art_index.insert(ff_index);
            }
            &mpv::Track::EmbeddedImage { ff_index, .. } => {
                if track_list.len() == 1 {
                    art_filename = path;
                } else {
                    _ = art_index.get_or_insert(ff_index);
                }
            }
            mpv::Track::None(_) => (),
        }
    }
    if let Some(file) = art_filename {
        let path = working_directory.as_ref().map(|dir| dir.join(file));
        let path = path.unwrap_or_else(|| file.clone());
        return Track::External(Url::from_file_path(path).ok()?).into();
    }
    if let Some(index) = art_index
        && let Some(path) = path
    {
        return Track::Embedded(path.clone(), index).into();
    }
    None
}
