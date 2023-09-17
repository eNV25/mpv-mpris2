#![allow(clippy::cast_possible_truncation)]

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

    let mut seeking = false;
    loop {
        let mut changed = BitFlags::default();

        for ev in iter::once(-1.0)
            .chain(iter::repeat(0.0))
            .map(|timeout| unsafe { *mpv_wait_event(ctx, timeout) })
        {
            match ev.event_id {
                MPV_EVENT_NONE => break,
                MPV_EVENT_SHUTDOWN => return 0,
                MPV_EVENT_SEEK => seeking = true,
                MPV_EVENT_PLAYBACK_RESTART if seeking => {
                    seeking = false;
                    if let Ok(position) = get!(ctx, "playback-time", f64) {
                        server
                            .emit(Signal::Seeked {
                                position: Time::from_micros((position * 1E6) as i64),
                            })
                            .await
                            .unwrap_or_else(elog);
                    }
                }
                MPV_EVENT_PROPERTY_CHANGE => {
                    let prop: mpv_event_property = unsafe { *ev.data.cast() };
                    let name = unsafe { CStr::from_ptr(prop.name) }
                        .to_str()
                        .unwrap_or_default();
                    if let Some(prop) = property(name) {
                        changed |= prop;
                    }
                    if prop.format == MPV_FORMAT_STRING && name == "keep-open" {
                        const EOF_REACHED: u64 = u64::from_ne_bytes(*b"mpvEOFed");
                        let value = unsafe { CStr::from_ptr(*prop.data.cast()) }
                            .to_str()
                            .unwrap_or_default();
                        if value == "no" {
                            unobserve!(ctx, EOF_REACHED);
                        } else {
                            observe!(ctx, EOF_REACHED, "eof-reached", MPV_FORMAT_NONE);
                        }
                    }
                }
                _ => {}
            }
        }

        server
            .properties_changed(changed)
            .await
            .unwrap_or_else(elog);
    }
}

fn property(prop: &str) -> Option<Property> {
    use Property::*;
    Some(match prop {
        "seekable" => CanSeek,
        "idle-active" | "keep-open" | "eof-reached" | "pause" => PlaybackStatus,
        "loop-file" | "loop-playlist" => LoopStatus,
        "speed" => Rate,
        "shuffle" => Shuffle,
        "metadata" | "media-title" | "duration" => Metadata,
        "volume" => Volume,
        "fullscreen" => Fullscreen,
        _ => return None,
    })
}
