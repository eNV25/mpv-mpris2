use crate::mpv::{self, Mpv};
use futures_concurrency::stream::Merge;
use mpris_server::Signal;
use futures_lite::prelude::*;
use zruntime::{LocalRuntime, lock::RwLock};
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
    runtime: &LocalRuntime,
    server: mpris_server::Server<Player>,
    events: kanal::AsyncReceiver<Vec<mpv::Event>>,
) -> anyhow::Result<()> {
    enum LoopEvent {
        MpvEvents(Vec<mpv::Event>),
        ArtFile(NamedTempFile),
    }
    let (mut art, art_files) = art::State::new();
    let mut events = {
        let events = events.stream().map(LoopEvent::MpvEvents);
        let art_files = art_files.stream().map(LoopEvent::ArtFile);
        (events, art_files).merge()
    };
    while let Some(loop_event) = events.next().await {
        let mut state = server.imp().state().await;
        let mut seeked = None;
        match loop_event {
            LoopEvent::MpvEvents(events) => {
                use mpv::{Event, Property};
                for event in events {
                    match event {
                        Event::Shutdown => return Ok(()),
                        Event::StartFile {
                            playlist_entry_id: value,
                        } => {
                            state.art_url = None;
                            state.playlist_entry_id = Some(value);
                            art.clear();
                        }
                        Event::EndFile => {
                            state.playlist_entry_id = None;
                        }
                        Event::PropertyChange(Property::Known(property)) => {
                            state.change(property);
                        }
                        // Every observed property is a `KnownProperty`, so this
                        // means its value didn't match the expected type.
                        Event::PropertyChange(Property::Unknown { name, data }) => {
                            tracing::error!(%name, %data, "Failed to parse property change");
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
        let changes = server.imp().update(state).await;
        if let Some((path, index)) = changes.embedded_art {
            art.spawn_worker(runtime, path, index);
        }
        if let Err(e) = server.properties_changed(changes.properties).await {
            tracing::error!(error = %e, "Failed to emit changes");
        }
    }
    Ok(())
}
