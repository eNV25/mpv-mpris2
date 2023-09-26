use std::{collections::HashMap, future::Future, process, sync::OnceLock};

use zbus::{
    blocking::{Connection, ConnectionBuilder},
    fdo,
    names::InterfaceName,
    zvariant::Value,
    SignalContext,
};

pub use zbus::{Task, block_on};

pub const OBJ_PATH: &str = "/org/mpris/MediaPlayer2";

static CONNECTION: OnceLock<Connection> = OnceLock::new();

pub fn connection(ctx: crate::Handle) -> zbus::Result<&'static Connection> {
    assert!(CONNECTION.get().is_none());
    let pid = process::id();
    let connection = ConnectionBuilder::session()?
        .name(format!("org.mpris.MediaPlayer2.mpv.instance{pid}"))?
        .serve_at(OBJ_PATH, crate::mpris2::Root(ctx))?
        .serve_at(OBJ_PATH, crate::mpris2::Player(ctx))?
        .build()?;
    CONNECTION.set(connection).unwrap();
    Ok(CONNECTION.get().unwrap())
}

pub fn spawn<T: Send + 'static>(
    future: impl Future<Output = T> + Send + 'static,
    name: &str,
) -> Task<T> {
    // spawn is undocumented, but IDGAF because I get to save on an extra background thread.
    CONNECTION
        .get()
        .unwrap()
        .inner()
        .executor()
        .spawn(future, name)
}

pub fn properties_changed(
    ctxt: &SignalContext<'_>,
    interface_name: InterfaceName<'_>,
    changed_properties: &HashMap<&str, Value<'_>>,
) -> zbus::Result<()> {
    block_on(fdo::Properties::properties_changed(
        ctxt,
        interface_name,
        &changed_properties.iter().map(|(&k, v)| (k, v)).collect(),
        &[],
    ))
}

pub fn seeked(ctxt: &SignalContext<'_>, position: i64) -> zbus::Result<()> {
    block_on(crate::mpris2::Player::seeked(ctxt, position))
}
