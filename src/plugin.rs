#![allow(clippy::not_unsafe_ptr_arg_deref)] // mpv_wait_event

use std::{
    collections::HashSet,
    ffi::{c_int, CStr},
    iter, process,
};

use crate::mpv::*;

mod mp;
mod mpris;
mod mpv;

#[no_mangle]
pub extern "C" fn mpv_open_cplugin(ctx: *mut mpv_handle) -> c_int {
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
    observe!(
        ctx,
        "seekable\0",
        "idle-active\0",
        "pause\0",
        "loop-file\0",
        "loop-playlist\0",
        "speed\0",
        "shuffle\0",
        "metadata\0",
        "volume\0",
        "fullscreen\0",
    );

    observe_format!(ctx, MPV_MPRIS, "keep-open\0", MPV_FORMAT_STRING);

    const EOF_REACHED: u64 = u64::from_ne_bytes(*b"mpvEOFed");

    let mut seeking = false;
    loop {
        let mut shutdown = false;
        let mut seeked = false;

        // We don't need to mpv_free() these strings or anything returned by mpv_wait_event(),
        // the API does it automatically.
        let changed: HashSet<String> = iter::once(-1.0)
            .chain(iter::repeat(0.0))
            .map(|timeout| unsafe { *mpv_wait_event(ctx, timeout) })
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
            .filter_map(|ev| match ev {
                mpv_event {
                    event_id: MPV_EVENT_PROPERTY_CHANGE,
                    reply_userdata: MPV_MPRIS | EOF_REACHED,
                    error: 0..,
                    data,
                } => match unsafe { *data.cast() } {
                    mpv_event_property {
                        format: MPV_FORMAT_NONE,
                        name,
                        ..
                    } => unsafe { CStr::from_ptr(name) }.to_str().ok().map(str::to_owned),
                    mpv_event_property {
                        format: MPV_FORMAT_STRING,
                        name,
                        data,
                    } => match (
                        unsafe { CStr::from_ptr(name) }.to_str(),
                        unsafe { CStr::from_ptr(*data.cast()) }.to_str(),
                    ) {
                        (Ok("keep-open"), Ok(value)) => {
                            if value == "no" {
                                unobserve!(ctx, EOF_REACHED);
                                None
                            } else {
                                observe_format!(ctx, EOF_REACHED, "eof-reached\0", MPV_FORMAT_NONE);
                                None
                            }
                        }
                        _ => None,
                    },
                    _ => None,
                },
                _ => None,
            })
            .collect();

        if shutdown {
            return 0;
        }

        if seeked {
            if let Ok(position) = get_float!(ctx, "playback-time\0") {
                _ = smol::block_on(mpris::PlayerImpl::seeked(
                    player.signal_context(),
                    (position * 1E6) as i64,
                ));
            }
        }

        macro_rules! signal_changed {
            ($iface:expr, $method:ident) => {
                smol::block_on(async {
                    _ = $iface.get().await.$method($iface.signal_context()).await;
                })
            };
        }

        macro_rules! forward_properties {
            ($changed:expr, $(($iface:expr, $method:ident, $prop0:expr $(, $propn:expr)* $(,)?),)+) => {
                $(if $changed.contains($prop0) $(|| $changed.contains($propn) )*{
                    signal_changed!($iface, $method);
                })+
            };
        }

        forward_properties!(
            changed,
            (player, can_seek_changed, "seekable"),
            (
                player,
                playback_status_changed,
                "idle-active",
                "eof-reached",
                "pause",
            ),
            (player, loop_status_changed, "loop-file", "loop-playlist"),
            (player, rate_changed, "speed"),
            (player, shuffle_changed, "shuffle"),
            (
                player,
                metadata_changed,
                "metadata",
                "media-title",
                "duration",
            ),
            (player, volume_changed, "volume"),
            (root, fullscreen_changed, "fullscreen"),
        );
    }
}
