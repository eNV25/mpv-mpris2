use std::{collections::HashMap, future::Future};

use zbus::{fdo, zvariant, object_server::SignalContext};

pub trait Block: Sized + Future {
    fn block_lite(self) -> <Self as Future>::Output {
        smol::future::block_on(self)
    }
    fn block(self) -> <Self as Future>::Output {
        smol::block_on(self)
    }
}

impl<F: Future> Block for F {}

pub fn properties_changed<I: zbus::object_server::Interface>(
    ctxt: &SignalContext<'_>,
    changed_properties: &HashMap<&str, &zvariant::Value<'_>>,
) -> zbus::Result<()> {
    fdo::Properties::properties_changed(ctxt, I::name(), changed_properties, &[]).block()
}

pub fn seeked(ctxt: &SignalContext<'_>, position: i64) -> zbus::Result<()> {
    crate::mpris2::Player::seeked(ctxt, position).block()
}
