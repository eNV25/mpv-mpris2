use crate::{mpv::Mpv, plugin::Player};
use compact_str::format_compact;
use mpris_server::Server;
use tracing_subscriber::{filter::Targets, prelude::*};
use zruntime::{LocalRuntime, net::unix::UnixStream};

mod future;
mod mpv;
mod plugin;

fn main() -> anyhow::Result<()> {
    // `Targets` handles `RUST_LOG=target=level,...` without `EnvFilter`'s
    // regex engine, which is about a sixth of the binary.
    let filter: Targets = std::env::var("RUST_LOG")
        .ok()
        .and_then(|directives| directives.parse().ok())
        .unwrap_or_else(|| Targets::new().with_default(tracing::Level::ERROR));
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(filter)
        .init();

    let runtime = LocalRuntime::new()?;
    runtime.block_on(async {
        let stream = UnixStream::from_std(&runtime, plugin::args::mpv_ipc_fd()?.into())?;
        let (mpv, handshake_tx) = Mpv::new(stream);

        let Some(pid): Option<usize> = mpv.get_property("pid").await? else {
            anyhow::bail!("No PID found");
        };

        let name = format_compact!("mpv.instance{}", pid);
        let server = Server::new(&name, Player::new(mpv).await?).await?;

        plugin::main_loop(&runtime, server, handshake_tx).await?;

        Ok(())
    })
}
