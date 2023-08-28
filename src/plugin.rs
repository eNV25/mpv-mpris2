#![allow(clippy::not_unsafe_ptr_arg_deref)] // mpv_wait_event

#[macro_use]
mod mp;

pub(crate) mod mpris;
pub(crate) mod mpv;

pub(crate) use crate::mpv::*;

use std::{collections, ffi, iter, process};

#[no_mangle]
pub extern "C" fn mpv_open_cplugin(ctx: *mut mpv_handle) -> ffi::c_int {
    if ctx.is_null() {
        return 1;
    }

    let pid = process::id();
    let connection = smol::block_on(async {
        let connection = zbus::ConnectionBuilder::session()?
            .name(format!("org.mpris.MediaPlayer2.mpv.instance{pid}"))?
            .serve_at("/org/mpris/MediaPlayer2", mpris::RootImpl::from(ctx))?
            .serve_at("/org/mpris/MediaPlayer2", mpris::PlayerImpl::from(ctx))?
            .build()
            .await?;
        zbus::Result::Ok(connection)
    })
    .expect("dbus session connection and server setup");

    let root: zbus::InterfaceRef<mpris::RootImpl> = smol::block_on(
        connection
            .object_server()
            .interface("/org/mpris/MediaPlayer2"),
    )
    .expect("MediaPlayer2 interface reference");
    let player: zbus::InterfaceRef<mpris::PlayerImpl> = smol::block_on(
        connection
            .object_server()
            .interface("/org/mpris/MediaPlayer2"),
    )
    .expect("MediaPlayer2.Player interface reference");

    // These properties and those handled in the main loop
    // must be kept in sync with the implementations in the
    // dbus interface implementations.
    // It's a bit of a pain in the ass but there's no other way.
    observe_properties!(
        ctx,
        "seekable\0",
        "idle-active\0",
        "keep-open\0",
        "eof-reached\0",
        "pause\0",
        "loop-file\0",
        "loop-playlist\0",
        "speed\0",
        "shuffle\0",
        "metadata\0",
        "volume\0",
        "fullscreen\0",
    );

    let mut keep_open;
    let mut seeking = false;
    loop {
        let mut shutdown = false;
        let mut seeked = false;

        // We don't need to mpv_free() these strings or anything returned by mpv_wait_event(),
        // the API does it automatically.
        let changed: collections::HashMap<&str, &str> = iter::once(-1.0)
            .chain(iter::repeat(0.0))
            .map(|timeout|
                // SAFETY: event cannot be NULL
                unsafe { mpv_wait_event(ctx, timeout).as_ref().unwrap_unchecked() })
            .take_while(|ev| match ev.event_id {
                MPV_EVENT_NONE => false,
                MPV_EVENT_SHUTDOWN => {
                    shutdown = true;
                    false
                }
                MPV_EVENT_SEEK => {
                    seeking = true;
                    true
                }
                MPV_EVENT_PLAYBACK_RESTART if seeking => {
                    seeking = false;
                    seeked = true;
                    true
                }
                _ => true,
            })
            .filter_map(|ev| {
                if ev.event_id != MPV_EVENT_PROPERTY_CHANGE || ev.reply_userdata != REPLY_USERDATA {
                    return None;
                }
                let prop: mpv_event_property = unsafe { *ev.data.cast() };
                let name = unsafe { ffi::CStr::from_ptr(prop.name) }.to_str();
                if prop.format != MPV_FORMAT_STRING {
                    return None;
                }
                let value = unsafe { ffi::CStr::from_ptr(*prop.data.cast()) }.to_str();
                match (name, value) {
                    (Ok(name), Ok(value)) => Some((name, value)),
                    _ => None,
                }
            })
            .collect();

        if shutdown {
            return 0;
        }

        if let Some(&"no") = changed.get("keep-open") {
            keep_open = false;
        } else {
            keep_open = true;
        }

        if seeked {
            let position = get_property_float!(ctx, "playback-time\0") * 1E6;
            _ = smol::block_on(mpris::PlayerImpl::seeked(
                player.signal_context(),
                position as i64,
            ));
        }

        macro_rules! signal_changed {
            ($iface:expr, $method:ident) => {
                smol::block_on(async {
                    _ = $iface.get().await.$method($iface.signal_context()).await;
                })
            };
        }

        macro_rules! forward_properties {
            ($changed:expr, $(($iface:expr, $method:ident, $prop0:expr $(, $prop1:expr)*),)+) => {
                match () {
                    $(_ if $changed.contains_key($prop0)$( || $changed.contains_key($prop1))* => {
                        signal_changed!($iface, $method);
                    })+
                    _ => {},
                }
            };
        }

        if keep_open && changed.contains_key("eof-reached") {
            signal_changed!(player, playback_status_changed);
        }

        forward_properties!(
            changed,
            (player, can_seek_changed, "seekable"),
            (player, playback_status_changed, "idle-active", "pause"),
            (player, loop_status_changed, "loop-file", "loop-playlist"),
            (player, rate_changed, "speed"),
            (player, shuffle_changed, "shuffle"),
            (
                player,
                metadata_changed,
                "metadata",
                "media-title",
                "duration"
            ),
            (player, volume_changed, "volume"),
            (root, fullscreen_changed, "fullscreen"),
        );
    }
}
