#![allow(clippy::not_unsafe_ptr_arg_deref)] // mpv_wait_event

use std::{
    ffi::{c_int, CStr},
    iter, process,
};

use mpris_server::{enumflags2::BitFlags, Property, Server, Signal, Time};

use crate::ffi::*;

mod ffi;
mod macros;
mod mpris2;

#[no_mangle]
pub extern "C" fn mpv_open_cplugin(ctx: *mut mpv_handle) -> c_int {
    if ctx.is_null() {
        return 1;
    }
    smol::block_on(plugin(ctx))
}

async fn plugin(ctx: *mut mpv_handle) -> c_int {
    let script = unsafe { CStr::from_ptr(mpv_client_name(ctx)) }
        .to_str()
        .unwrap_or_default();
    let elog = |err| eprintln!("[{script}] {err}");

    let pid = process::id();
    let server = Server::new(
        &format!("mpv.instance{pid}"),
        mpris2::Player(crate::Handle(ctx)),
    )
    .expect("MPRIS server");

    // These properties and those handled in the main loop
    // must be kept in sync with the implementations in the
    // dbus interface implementations.
    // It's a bit of a pain in the ass but there's no other way.
    observe!(
        ctx,
        "seekable",
        "idle-active",
        "pause",
        "loop-file",
        "loop-playlist",
        "speed",
        "shuffle",
        "metadata",
        "media-title",
        "duration",
        "volume",
        "fullscreen",
    );

    observe!(ctx, "keep-open", MPV_FORMAT_STRING);

    const EOF_REACHED: u64 = u64::from_ne_bytes(*b"mpvEOFed");

    let mut seeking = false;
    loop {
        let mut shutdown = false;
        let mut seeked = false;

        let changed: BitFlags<Property> = iter::once(-1.0)
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
                    error: 0..,
                    data,
                    ..
                } => match unsafe { *data.cast() } {
                    mpv_event_property {
                        format: MPV_FORMAT_NONE,
                        name,
                        ..
                    } => match unsafe { CStr::from_ptr(name) }.to_str() {
                        Ok(name) => property(name),
                        _ => None,
                    },
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
                            } else {
                                observe!(ctx, EOF_REACHED, "eof-reached", MPV_FORMAT_NONE);
                            }
                            property("keep-open")
                        }
                        _ => None,
                    },
                    _ => None,
                },
                _ => None,
            })
            .collect();

        server
            .properties_changed(changed)
            .await
            .unwrap_or_else(elog);

        if seeked {
            if let Ok(position) = get!(ctx, "playback-time", f64) {
                server
                    .emit(Signal::Seeked {
                        position: Time::from_micros((position * 1E6) as i64),
                    })
                    .await
                    .unwrap_or_else(elog);
            }
        }

        if shutdown {
            return 0;
        }
    }
}

const fn property(prop: &str) -> Option<Property> {
    use Property::*;
    Some(match prop.as_bytes() {
        b"seekable" => CanSeek,
        b"idle-active" | b"keep-open" | b"eof-reached" | b"pause" => PlaybackStatus,
        b"loop-file" | b"loop-playlist" => LoopStatus,
        b"speed" => Rate,
        b"shuffle" => Shuffle,
        b"metadata" | b"media-title" | b"duration" => Metadata,
        b"volume" => Volume,
        b"fullscreen" => Fullscreen,
        _ => return None,
    })
}
