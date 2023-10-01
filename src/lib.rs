use std::{collections::HashMap, future::Future};

use zbus::{fdo, names::InterfaceName, zvariant::Value, SignalContext};

mod block {
    pub trait Sealed {}
}

pub trait Block<T>: block::Sealed {
    fn block(self) -> T;
    fn block_io(self) -> T;
}

impl<F: Future<Output = T>, T> block::Sealed for F {}
impl<F: Future<Output = T>, T> Block<T> for F {
    fn block(self) -> T {
        futures_lite::future::block_on(self)
    }
    fn block_io(self) -> T {
        async_io::block_on(self)
    }
}

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
