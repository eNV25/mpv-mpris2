use self::ipc::MpvIpcWorker;
use serde::{Serialize, de::DeserializeOwned};
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
        let (requests_tx, requests) = kanal::bounded_async(0);
        let (events_tx_tx, events_tx) = oneshot::async_channel();
        ex.spawn(MpvIpcWorker::new(stream, requests, events_tx).run())
            .detach();
        (Self { requests_tx }, events_tx_tx)
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
        self.run_command(command).await
    }

    pub(crate) async fn observe_property(&self, name: impl Into<&'static str>) -> Result<()> {
        let command = ListCommand::ObserveProperty(0, name.into());
        self.run_command(command).await
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
