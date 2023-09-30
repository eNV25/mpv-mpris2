use std::collections::HashMap;

use async_io::block_on;
use zbus::{fdo, names::InterfaceName, zvariant::Value, SignalContext};

pub fn properties_changed(
    ctxt: &SignalContext<'_>,
    interface_name: InterfaceName<'_>,
    changed_properties: &HashMap<&str, &Value<'_>>,
) -> zbus::Result<()> {
    block_on(fdo::Properties::properties_changed(
        ctxt,
        interface_name,
        changed_properties,
        &[],
    ))
}

pub fn seeked(ctxt: &SignalContext<'_>, position: i64) -> zbus::Result<()> {
    block_on(crate::mpris2::Player::seeked(ctxt, position))
}
