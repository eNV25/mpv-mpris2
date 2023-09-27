use std::{collections::HashMap, future::Future, process, sync::OnceLock};

use zbus::{
    blocking::{Connection, ConnectionBuilder},
    fdo,
    names::InterfaceName,
    zvariant::Value,
    SignalContext,
};

pub use zbus::{block_on, Task};

pub const OBJ_PATH: &str = "/org/mpris/MediaPlayer2";

static mut CONNECTION: OnceLock<Connection> = OnceLock::new();

pub fn connect(ctx: crate::Handle) -> zbus::Result<&'static Connection> {
    assert!(unsafe { CONNECTION.get() }.is_none());
    let pid = process::id();
    let connection = ConnectionBuilder::session()?
        .name(format!("org.mpris.MediaPlayer2.mpv.instance{pid}"))?
        .serve_at(OBJ_PATH, crate::mpris2::Root(ctx))?
        .serve_at(OBJ_PATH, crate::mpris2::Player(ctx))?
        .build()?;
    unsafe { CONNECTION.set(connection).unwrap_unchecked() };
    Ok(unsafe { CONNECTION.get().unwrap_unchecked() })
}

pub fn disconnect() {
    unsafe {
        CONNECTION.take();
    }
}

pub fn spawn<T: Send + 'static>(
    future: impl Future<Output = T> + Send + 'static,
    name: &str,
) -> Task<T> {
    // spawn is undocumented, but IDGAF because I get to save on an extra background thread.
    unsafe { CONNECTION.get().unwrap_unchecked() }
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
