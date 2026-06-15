use crate::mpv::{self, Mpv};
use futures_concurrency::stream::Merge;
use mpris_server::Signal;
use smol::{LocalExecutor, lock::RwLock, prelude::*};
use tempfile::NamedTempFile;
use url::Url;

pub(crate) mod args;
mod art;
mod mpris;
mod state;

pub(crate) struct Player {
    state: RwLock<state::State>,
    mpv: Mpv,
}

pub(crate) async fn main_loop(
    ex: &LocalExecutor<'_>,
    server: mpris_server::Server<Player>,
    events_tx: oneshot::Sender<kanal::AsyncSender<Vec<mpv::Event>>>,
) -> anyhow::Result<()> {
    enum LoopEvent {
        Events(Vec<mpv::Event>),
        ArtFile(NamedTempFile),
    }
    let events = kanal::bounded_async(0);
    let (mut art, art_files) = art::State::new();
    let mut events = {
        events_tx.send(events.0)?;
        (
            events.1.stream().map(LoopEvent::Events),
            art_files.stream().map(LoopEvent::ArtFile),
        )
            .merge()
    };
    while let Some(loop_event) = events.next().await {
        let mut state = server.imp().state().await;
        let mut seeked = None;
        match loop_event {
            LoopEvent::Events(events) => {
                use mpv::{Event, Property};
                for event in events {
                    match event {
                        Event::Shutdown => return Ok(()),
                        Event::StartFile {
                            playlist_entry_id: value,
                        } => {
                            state.art_url = None;
                            art.clear();
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
            LoopEvent::ArtFile(file) => {
                state.art_url = Url::from_file_path(file.path()).ok();
                art.set_file(file);
            }
        }
        if let Some(playback_time) = seeked.take()
            && let Err(e) = server
                .emit(Signal::Seeked {
                    position: playback_time.into(),
                })
                .await
        {
            tracing::error!(error = %e, "Failed to emit seeked signal");
        }
        let changes = server.imp().update(&mut state).await;
        if let Some((path, index)) = state.art_index.take() {
            art.spawn_worker(ex, path, index);
        }
        if let Err(e) = changes.emit(server.connection()).await {
            tracing::error!(error = %e, "Failed to emit changes");
        }
    }
    Ok(())
}
