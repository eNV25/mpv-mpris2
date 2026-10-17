use self::ipc::MpvIpcWorker;
use serde::{
    Serialize,
    de::{DeserializeOwned, IgnoredAny},
};
use smol::{LocalExecutor, net::unix::UnixStream};
use std::fmt::Debug;
use thiserror::Error;
use zbus::fdo;

mod ipc;
mod protocol;

pub(crate) use protocol::*;

#[derive(Clone)]
pub(crate) struct Mpv {
    requests_tx: kanal::AsyncSender<(Command, oneshot::Sender<Result<serde_json::Value, String>>)>,
}

impl Mpv {
    pub(crate) fn new(
        ex: &LocalExecutor,
        stream: UnixStream,
    ) -> (Self, oneshot::Sender<kanal::AsyncSender<Vec<Event>>>) {
        let (requests_tx, requests_rx) = kanal::bounded_async(0);
        let (handshake_tx, handshake_rx) = oneshot::async_channel();
        ex.spawn(MpvIpcWorker::new(stream, requests_rx, handshake_rx).run())
            .detach();
        (Self { requests_tx }, handshake_tx)
    }

    pub(crate) fn subscribe(
        handshake_tx: oneshot::Sender<kanal::AsyncSender<Vec<Event>>>,
    ) -> anyhow::Result<kanal::AsyncReceiver<Vec<Event>>> {
        let (events_tx, events) = kanal::bounded_async(0);
        handshake_tx.send(events_tx)?;
        Ok(events)
    }

    pub(crate) async fn run_command<T>(&self, command: impl Into<Command>) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let (request, response) = oneshot::async_channel();
        self.requests_tx.send((command.into(), request)).await?;
        let value = response.await?.map_err(Error::Mpv)?;
        Ok(serde_json::from_value(value)?)
    }

    /// Runs a command and ignores whatever it returns. Some commands return
    /// data even when the caller doesn't need it, e.g. `loadfile` returns
    /// `{"playlist_entry_id": N}`.
    pub(crate) async fn command(&self, command: impl Into<Command>) -> Result<()> {
        self.run_command::<IgnoredAny>(command).await?;
        Ok(())
    }

    pub(crate) async fn get_property<T>(&self, name: impl Into<&'static str>) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let command = ListCommand::GetProperty(name.into());
        self.run_command(command).await
    }

    pub(crate) async fn set_property(
        &self,
        name: impl Into<&'static str>,
        value: impl Serialize,
    ) -> Result<()> {
        let value = serde_json::to_value(value)?;
        let command = ListCommand::SetProperty(name.into(), value);
        self.command(command).await
    }

    pub(crate) async fn observe_property(&self, name: impl Into<&'static str>) -> Result<()> {
        let command = ListCommand::ObserveProperty(0, name.into());
        self.command(command).await
    }
}

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error("MPV JSON IPC error: {0}")]
    Mpv(String),
    #[error(transparent)]
    Kanal(#[from] kanal::SendError),
    #[error(transparent)]
    Oneshot(#[from] oneshot::RecvError),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

impl Error {
    /// mpv's reply when reading a property that has no value right now, e.g.
    /// `playback-time` while nothing is loaded.
    pub(crate) fn is_property_unavailable(&self) -> bool {
        matches!(self, Self::Mpv(error) if error == "property unavailable")
    }
}

impl From<Error> for fdo::Error {
    fn from(value: Error) -> Self {
        Self::Failed(value.to_string())
    }
}

impl From<Error> for zbus::Error {
    fn from(value: Error) -> Self {
        Self::Failure(value.to_string())
    }
}
