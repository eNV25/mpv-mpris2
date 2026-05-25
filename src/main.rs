use crate::{mpv::Mpv, plugin::Player};
use mpris_server::Server;
use smol::LocalExecutor;
use tracing_subscriber::EnvFilter;

mod common;
mod mpv;
mod plugin;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let ex = LocalExecutor::new();
    smol::block_on(ex.run(async {
        let (mpv, events_tx) = Mpv::new(&ex, plugin::args::mpv_ipc_fd()?.try_into()?);

        let Some(pid): Option<usize> = mpv.get_property("pid").await? else {
            anyhow::bail!("No PID found");
        };

        let name = format!("mpv.instance{}", pid);
        let server = Server::new(&name, Player::new(mpv).await?).await?;

        plugin::main_loop(&ex, server, events_tx).await?;

        Ok(())
    }))
}
