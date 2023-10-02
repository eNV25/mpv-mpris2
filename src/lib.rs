use std::{collections::HashMap, future::Future};

use zbus::{fdo, names::InterfaceName, zvariant::Value, SignalContext};

mod block {
    pub trait Sealed {}
}

pub trait Block: Sized + Future + block::Sealed {
    fn block(self) -> <Self as Future>::Output {
        futures_lite::future::block_on(self)
    }
    fn block_io(self) -> <Self as Future>::Output {
        async_io::block_on(self)
    }
}

impl<F: Future> block::Sealed for F {}
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
