use crate::{
    common::time_from_secs,
    mpv::{Event, Mpv, Path, Property, Track},
};
use data_encoding::BASE64;
use futures_concurrency::stream::Merge;
use mpris_server::Signal;
use smol::{LocalExecutor, lock::RwLock, prelude::*, process::Command};
use std::{path::PathBuf, process::Stdio};
use url::Url;

pub(crate) mod args;
mod mpris;
mod state;

pub(crate) struct Player {
    state: RwLock<state::State>,
    mpv: Mpv,
}

pub(crate) async fn main_loop(
    ex: &LocalExecutor<'_>,
    server: mpris_server::Server<Player>,
    events_tx: oneshot::Sender<kanal::AsyncSender<Vec<Event>>>,
) -> anyhow::Result<()> {
    enum LoopEvent {
        Events(Vec<Event>),
        ArtUrl(Url),
    }
    let events = kanal::bounded_async(0);
    let art_urls = kanal::bounded_async(0);
    let mut art_task = None;
    let mut events = {
        events_tx.send(events.0)?;
        (
            events.1.stream().map(LoopEvent::Events),
            art_urls.1.stream().map(LoopEvent::ArtUrl),
        )
            .merge()
    };
    while let Some(loop_event) = events.next().await {
        let mut state = server.imp().state().await;
        let mut seeked = None;
        match loop_event {
            LoopEvent::Events(events) => {
                for event in events {
                    match event {
                        Event::Shutdown => return Ok(()),
                        Event::StartFile {
                            playlist_entry_id: value,
                        } => {
                            state.art_url = None;
                            drop(art_task.take());
                            state.playlist_entry_id = Some(value);
                        }
                        Event::EndFile {
                            playlist_entry_id: _,
                            ..
                        } => {
                            state.playlist_entry_id = None;
                        }
                        Event::PropertyChange(Property::Known(property)) => {
                            state.change(property);
                        }
                        Event::Seeked { playback_time } => {
                            seeked = Some(playback_time);
                        }
                        _ => (),
                    }
                }
            }
            LoopEvent::ArtUrl(art_url) => {
                if state.art_url.is_none() {
                    state.art_url = Some(art_url);
                }
            }
        }
        if let Some(playback_time) = seeked.take()
            && let Err(e) = server
                .emit(Signal::Seeked {
                    position: time_from_secs(playback_time),
                })
                .await
        {
            tracing::error!(error = %e, "Failed to emit seeked signal");
        }
        let changes = server.imp().update(&mut state).await;
        if let Some((path, index)) = state.art_index.take() {
            let tx = art_urls.0.clone();
            art_task = Some(ex.spawn(art_worker(tx, path, index)));
        }
        if let Err(e) = changes.emit(server.connection()).await {
            tracing::error!(error = %e, "Failed to emit changes");
        }
    }
    Ok(())
}

async fn art_worker(tx: kanal::AsyncSender<Url>, path: PathBuf, index: u64) -> Option<()> {
    let url = Command::new("ffmpeg")
        .arg("-i")
        .arg(&path)
        .arg("-map")
        .arg(format!("0:{index}"))
        .args(["-c", "copy", "-f", "image2pipe", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .ok()
        .and_then(|output| {
            let mime_type = infer::get(&output.stdout)?.mime_type();
            let mut data = ["data:", mime_type, ";base64,"].concat();
            BASE64.encode_append(&output.stdout, &mut data);
            match Url::parse(&data) {
                Ok(url) => Some(url),
                Err(e) => {
                    tracing::error!(error = %e, "Failed to extract art from {index}@{path:?}");
                    None
                }
            }
        })?;
    if let Err(e) = tx.send(url).await {
        tracing::error!(error = %e, "Failed to send art url");
    }
    Some(())
}

#[derive(Debug, PartialEq, Eq)]
enum ArtInfo {
    Embedded(PathBuf, u64),
    External(Url),
}

fn art_info(
    track_list: &[Track],
    path: &Option<Path>,
    working_directory: &Option<PathBuf>,
) -> Option<ArtInfo> {
    let path = path.as_ref().and_then(|x| match x {
        Path::Path(path) => Some(path),
        _ => None,
    });
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
