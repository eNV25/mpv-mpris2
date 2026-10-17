use super::protocol::{Command, Event, ListCommand, Request, Response};
use futures_concurrency::stream::Merge;
use futures_lite::{
    io::{self, BufReader},
    prelude::*,
    stream,
};
use slab::Slab;
use std::{future, task::Poll};
use zruntime::net::unix::UnixStream;

/// Where the reply to a command is sent.
pub(super) type ReplySender = oneshot::Sender<Result<serde_json::Value, String>>;

/// Request ID of the `playback-time` query sent when a seek finishes. Its
/// reply becomes an [`Event::Seeked`]. Command request IDs are slab keys, so
/// they are never negative.
const SEEKED_REQUEST_ID: i64 = i64::MIN;

pub(super) struct MpvIpcWorker {
    stream: UnixStream,
    requests: kanal::AsyncReceiver<(Command, ReplySender)>,
    events: kanal::AsyncSender<Vec<Event>>,
}

impl MpvIpcWorker {
    pub(super) fn new(
        stream: UnixStream,
        requests: kanal::AsyncReceiver<(Command, ReplySender)>,
        events: kanal::AsyncSender<Vec<Event>>,
    ) -> Self {
        Self {
            stream,
            requests,
            events,
        }
    }

    pub(super) async fn run(self) {
        enum WorkerEvent {
            Responses(Vec<Response>),
            Command((Command, ReplySender)),
        }

        // Collects every line that can be read without waiting, so events that
        // arrive together are handled together.
        fn batch_ready_responses<T>(
            mut state: Option<T>,
        ) -> impl Future<Output = Option<(WorkerEvent, Option<T>)>>
        where
            T: Stream<Item = io::Result<Vec<u8>>> + Unpin,
        {
            future::poll_fn(move |cx| {
                let mut responses: Vec<Response> = Vec::new();
                let Some(stream) = state.as_mut() else {
                    return Poll::Ready(None);
                };
                let e = loop {
                    match stream.poll_next(cx) {
                        Poll::Ready(Some(Ok(line))) => {
                            // don't error out on invalid UTF8
                            let line = String::from_utf8_lossy(&line);
                            match serde_json::from_str(&line) {
                                Ok(response) => responses.push(response),
                                Err(e) => {
                                    tracing::error!(error = %e, "Failed to parse JSON IPC response");
                                }
                            }
                        }
                        Poll::Ready(Some(Err(e))) => break Some(e),
                        Poll::Ready(None) => break None,
                        Poll::Pending if responses.is_empty() => return Poll::Pending,
                        Poll::Pending => {
                            let responses = WorkerEvent::Responses(responses);
                            return Poll::Ready(Some((responses, state.take())));
                        }
                    }
                };
                if let Some(e) = e {
                    tracing::error!(error = %e, "Failed to read IPC response");
                }
                responses.push(Response::Event(Event::Shutdown));
                Poll::Ready(Some((WorkerEvent::Responses(responses), None)))
            })
        }

        let (reader, mut writer) = io::split(self.stream);
        let mut worker_events = {
            let lines = BufReader::new(reader).split(b'\n');
            let responses = stream::unfold(Some(lines), batch_ready_responses);
            let requests = self.requests.stream().map(WorkerEvent::Command);
            (responses, requests).merge()
        };
        // Commands waiting for a reply, keyed by request ID.
        let mut pending: Slab<ReplySender> = Slab::new();

        let mut seeking = false;
        while let Some(worker_event) = worker_events.next().await {
            match worker_event {
                WorkerEvent::Responses(responses) => {
                    let mut events = Vec::with_capacity(responses.len());
                    for response in responses {
                        match response {
                            Response::CommandResponseSuccess {
                                data, request_id, ..
                            } => {
                                if request_id == SEEKED_REQUEST_ID {
                                    if let Ok(playback_time) = serde_json::from_value(data) {
                                        events.push(Event::Seeked { playback_time });
                                    }
                                } else if let Some(sender) = pending.try_remove(request_id as _)
                                    && !sender.is_closed()
                                    && let Err(e) = sender.send(Ok(data))
                                {
                                    tracing::error!(error = %e, "Failed to send command reply");
                                }
                            }
                            Response::CommandResponseFailure { request_id, error } => {
                                if request_id == SEEKED_REQUEST_ID {
                                    tracing::error!(error = %error, "Failed get playback-time after seek event");
                                } else if let Some(sender) = pending.try_remove(request_id as _)
                                    && !sender.is_closed()
                                    && let Err(e) = sender.send(Err(error.into()))
                                {
                                    tracing::error!(error = %e, "Failed to send command reply");
                                }
                            }
                            Response::Event(event) => {
                                match event {
                                    Event::Seek => seeking = true,
                                    Event::PlaybackRestart if seeking => {
                                        seeking = false;
                                        let request = Request {
                                            command: ListCommand::GetProperty("playback-time")
                                                .into(),
                                            request_id: SEEKED_REQUEST_ID,
                                            r#async: Default::default(),
                                        };
                                        if let Err(e) = send_request(&mut writer, &request).await {
                                            tracing::error!(error = %e, "Failed to send IPC request");
                                        }
                                    }
                                    _ => (),
                                }
                                events.push(event);
                            }
                            Response::UnknownEvent { event } => {
                                tracing::debug!(%event, "Ignoring unknown event");
                            }
                        }
                    }
                    if !events.is_empty()
                        && let Err(e) = self.events.send(events).await
                    {
                        tracing::error!(error = %e, "Failed to send MPV events");
                    }
                }
                WorkerEvent::Command((command, sender)) => {
                    if !sender.is_closed() {
                        let entry = pending.vacant_entry();
                        let request = Request {
                            command,
                            request_id: entry.key() as _,
                            r#async: Default::default(),
                        };
                        if let Err(e) = send_request(&mut writer, &request).await {
                            tracing::error!(error = %e, "Failed to send IPC request");
                            if let Err(e) = sender.send(Err("Failed to send IPC command".into())) {
                                tracing::error!(error = %e, "Failed to send command reply");
                            }
                        } else {
                            entry.insert(sender);
                        }
                    }
                }
            }
        }
    }
}

async fn send_request(w: &mut (impl AsyncWrite + Unpin), request: &Request) -> io::Result<()> {
    let mut msg = serde_json::to_vec(request)?;
    msg.push(b'\n');
    w.write_all(&msg).await?;
    w.flush().await
}
