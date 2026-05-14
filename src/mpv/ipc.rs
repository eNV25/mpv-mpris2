use super::protocol::{Command, Event, ListCommand, Request, Response};
use futures_concurrency::stream::Merge;
use slab::Slab;
use smol::{io::BufReader, net::unix::UnixStream, prelude::*, stream};
use std::{
    future,
    io::{self, IoSlice},
    mem,
    task::Poll,
};

pub(super) struct MpvIpcWorker {
    stream: UnixStream,
    requests: kanal::AsyncReceiver<(Command, oneshot::Sender<Result<serde_json::Value, String>>)>,
    events_tx: oneshot::AsyncReceiver<kanal::AsyncSender<Vec<Event>>>,
}

impl MpvIpcWorker {
    pub(super) fn new(
        stream: UnixStream,
        requests: kanal::AsyncReceiver<(
            Command,
            oneshot::Sender<Result<serde_json::Value, String>>,
        )>,
        events_tx: oneshot::AsyncReceiver<kanal::AsyncSender<Vec<Event>>>,
    ) -> Self {
        Self {
            stream,
            requests,
            events_tx,
        }
    }

    pub(super) async fn run(mut self) {
        enum WorkerEvent {
            EventsSender(Result<kanal::AsyncSender<Vec<Event>>, oneshot::RecvError>),
            Responses(Vec<Response>),
            Command((Command, oneshot::Sender<Result<serde_json::Value, String>>)),
        }

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

        let mut stream = {
            let events_tx = stream::once_future(future::poll_fn(move |cx| {
                self.events_tx.poll(cx).map(WorkerEvent::EventsSender)
            }));
            let lines = BufReader::new(self.stream.clone()).split(b'\n');
            let responses = stream::unfold(Some(lines), batch_ready_responses);
            let requests = self.requests.stream().map(WorkerEvent::Command);
            (events_tx, responses, requests).merge()
        };
        let mut requests: Slab<oneshot::Sender<Result<serde_json::Value, String>>> = Slab::new();
        let mut events = Vec::new();
        let mut events_tx = None;

        let mut seeking = false;
        while let Some(worker_event) = stream.next().await {
            let mut seeked = false;
            match worker_event {
                WorkerEvent::EventsSender(Ok(new_events_tx)) => {
                    events_tx = Some(new_events_tx);
                }
                WorkerEvent::EventsSender(Err(e)) => {
                    tracing::error!(error = %e, "Failed to receive events sender");
                }
                WorkerEvent::Responses(responses) => {
                    events.reserve(responses.len());
                    for response in responses {
                        match response {
                            Response::CommandResponseSuccess {
                                data, request_id, ..
                            } => {
                                if request_id == i64::MIN {
                                    if let Ok(playback_time) = serde_json::from_value(data) {
                                        events.push(Event::Seeked { playback_time });
                                    }
                                } else if let Some(sender) = requests.try_remove(request_id as _)
                                    && !sender.is_closed()
                                    && let Err(e) = sender.send(Ok(data))
                                {
                                    tracing::error!(error = %e, "Failed to send command reply");
                                }
                            }
                            Response::CommandResponseFailure { request_id, error } => {
                                if let Some(sender) = requests.try_remove(request_id as _)
                                    && !sender.is_closed()
                                    && let Err(e) = sender.send(Err(error))
                                {
                                    tracing::error!(error = %e, "Failed to send command reply");
                                }
                            }
                            Response::Event(event) => {
                                match event {
                                    Event::Seek => seeking = true,
                                    Event::PlaybackRestart if seeking => {
                                        (seeking, seeked) = (false, true);
                                    }
                                    _ => (),
                                }
                                events.push(event);
                            }
                            Response::UnknownEvent { event } => {
                                events.push(Event::Unknown(event));
                            }
                        }
                    }
                }
                WorkerEvent::Command((command, sender)) => {
                    if !sender.is_closed() {
                        let entry = requests.vacant_entry();
                        let request = Request {
                            command,
                            request_id: entry.key() as _,
                            r#async: Default::default(),
                        };
                        if let Err(e) = send_request(&mut self.stream, request).await {
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
            if seeked {
                let request = Request {
                    command: ListCommand::GetProperty("playback-time").into(),
                    request_id: i64::MIN,
                    r#async: Default::default(),
                };
                if let Err(e) = send_request(&mut self.stream, request).await {
                    tracing::error!(error = %e, "Failed to send IPC request");
                }
            }
            if !events.is_empty()
                && let Some(event_tx) = &events_tx
                && let events = mem::take(&mut events)
                && let Err(e) = event_tx.send(events).await
            {
                tracing::error!(error = %e, "Failed to send MPV event");
            }
        }
        if let Some(events_tx) = events_tx.take()
            && let Err(e) = events_tx.close()
        {
            tracing::error!(error = %e, "Failed to close MPV events sender");
        }
    }
}

async fn send_request(w: &mut (impl AsyncWrite + Unpin), request: Request) -> io::Result<()> {
    let msg = match serde_json::to_vec(&request) {
        Ok(msg) => msg,
        Err(e) => return Err(io::Error::new(io::ErrorKind::InvalidData, e)),
    };
    let mut bufs = &mut [IoSlice::new(&msg), IoSlice::new(b"\n")][..];
    while !bufs.is_empty() {
        let n = w.write_vectored(bufs).await?;
        if n == 0 {
            return Err(io::ErrorKind::WriteZero.into());
        }
        IoSlice::advance_slices(&mut bufs, n);
    }
    w.flush().await?;
    Ok(())
}
