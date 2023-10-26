use std::{collections::HashMap, future::Future};

use zbus::{fdo, names::InterfaceName, zvariant::Value, SignalContext};

pub trait Block: Sized + Future {
    fn block(self) -> <Self as Future>::Output {
        smol::future::block_on(self)
    }
    fn block_io(self) -> <Self as Future>::Output {
        smol::block_on(self)
    }
}

impl<F: Future> Block for F {}

pub fn properties_changed(
    ctxt: &SignalContext<'_>,
    interface_name: InterfaceName<'_>,
    changed_properties: &HashMap<&str, &Value<'_>>,
) -> zbus::Result<()> {
    fdo::Properties::properties_changed(ctxt, interface_name, changed_properties, &[]).block_io()
}

pub fn seeked(ctxt: &SignalContext<'_>, position: i64) -> zbus::Result<()> {
    crate::mpris2::Player::seeked(ctxt, position).block_io()
}
