use std::{borrow::Cow, collections::HashMap, future::Future};

use zbus::{fdo, object_server::SignalEmitter, zvariant};

pub trait Block: Sized + Future {
    //fn block_lite(self) -> <Self as Future>::Output {
    //    smol::future::block_on(self)
    //}
    fn block(self) -> <Self as Future>::Output {
        smol::block_on(self)
    }
}

impl<F: Future> Block for F {}

pub fn properties_changed<I: zbus::object_server::Interface>(
    emitter: &SignalEmitter<'_>,
    changed_properties: HashMap<&str, zvariant::Value<'_>>,
) -> zbus::Result<()> {
    if changed_properties.is_empty() {
        Ok(())
    } else {
        fdo::Properties::properties_changed(emitter, I::name(), changed_properties, Cow::default())
            .block()
    }
}

pub fn seeked(emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()> {
    crate::mpris2::Player::seeked(emitter, position).block()
}
