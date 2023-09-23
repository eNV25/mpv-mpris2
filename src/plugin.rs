#![warn(clippy::suspicious)]
#![warn(clippy::complexity)]
#![warn(clippy::perf)]
#![warn(clippy::pedantic)]
#![allow(special_module_name)]

use std::{
    collections::HashMap,
    ffi::{c_int, CStr},
    iter,
};

use zbus::{zvariant::Value, Interface, SignalContext};

#[allow(clippy::wildcard_imports)]
use crate::ffi::*;
#[allow(clippy::wildcard_imports)]
use crate::lib::*;

mod ffi;
mod lib;
mod macros;
mod mpris2;

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn mpv_open_cplugin(ctx: *mut mpv_handle) -> c_int {
    if ctx.is_null() {
        return 1;
    }
    let script = unsafe { CStr::from_ptr(mpv_client_name(ctx)) }
        .to_str()
        .unwrap_or_default();
    match plugin(Handle(ctx), script) {
        Ok(_) => 0,
        Err(err) => {
            eprintln!("[{script}] {err:?}");
            1
        }
    }
}

#[allow(clippy::too_many_lines)]
fn plugin(ctx: Handle, name: &str) -> anyhow::Result<()> {
    let (root_ctxt, player_ctxt) = {
        let object_server = connection(ctx)?.object_server();
        (
            object_server
                .interface::<_, mpris2::Root>(OBJ_PATH)?
                .signal_context()
                .to_owned(),
            object_server
                .interface::<_, mpris2::Player>(OBJ_PATH)?
                .signal_context()
                .to_owned(),
        )
    };

    // These properties and those handled in the main loop must be kept in sync with those
    // mentioned in the interface implementations.
    // It's a bit of a pain in the ass but there's no other way.
    observe!(ctx, "media-title", "metadata", "duration");
    observe!(
        ctx,
        MPV_FORMAT_FLAG,
        "fullscreen",
        "seekable",
        "idle-active",
        // "eof-reached" is set in the main-loop
        "pause",
        "shuffle",
    );
    observe!(
        ctx,
        MPV_FORMAT_STRING,
        "keep-open",
        "loop-file",
        "loop-playlist",
    );
    observe!(ctx, MPV_FORMAT_DOUBLE, "speed", "volume");

    let elog = |err| eprintln!("[{name}] {err:?}");

    let mut seeking = false;
    let mut root_changed: HashMap<&'static str, Value<'static>> = HashMap::new();
    let mut player_changed: HashMap<&'static str, Value<'static>> = HashMap::new();
    loop {
        let mut state = State::default();

        for ev in iter::once(-1.0)
            .chain(iter::repeat(0.0))
            .map(|timeout| unsafe { *mpv_wait_event(ctx.into(), timeout) })
        {
            macro_rules! data {
                ($source:expr, bool) => {
                    data!($source, std::ffi::c_int) != 0
                };
                ($source:expr, String) => {
                    unsafe { CStr::from_ptr(*$source.data.cast()) }
                        .to_str()
                        .unwrap_or_default()
                        .to_owned()
                };
                ($source:expr, $type:ty) => {
                    unsafe { *$source.data.cast::<$type>() }
                };
            }
            match ev.event_id {
                MPV_EVENT_NONE => break,
                MPV_EVENT_SHUTDOWN => {
                    signal_changed(
                        ctx,
                        state,
                        (&root_ctxt, &mut root_changed),
                        (&player_ctxt, &mut player_changed),
                    )
                    .for_each(|err| err.unwrap_or_else(elog));
                    return Ok(());
                }
                MPV_EVENT_SEEK => seeking = true,
                MPV_EVENT_PLAYBACK_RESTART if seeking => {
                    seeking = false;
                    if let Ok(position) = get!(ctx, "playback-time", f64) {
                        seeked(&player_ctxt, mpris2::time_from_secs(position)).unwrap_or_else(elog);
                    }
                }
                MPV_EVENT_PROPERTY_CHANGE => {
                    let prop = data!(ev, mpv_event_property);
                    let name = unsafe { CStr::from_ptr(prop.name) }
                        .to_str()
                        .unwrap_or_default();
                    match (name, prop.format) {
                        ("media-title" | "metadata" | "duration", _) => {
                            state.metadata = true;
                        }
                        ("keep-open", MPV_FORMAT_STRING) => {
                            const EOF_REACHED: u64 = u64::from_ne_bytes(*b"mpvEOFed");
                            let value = unsafe { CStr::from_ptr(*prop.data.cast()) }
                                .to_str()
                                .unwrap_or_default();
                            state.keep_open.replace(if value == "no" {
                                unobserve!(ctx, EOF_REACHED);
                                false
                            } else {
                                observe!(ctx, EOF_REACHED, "eof-reached", MPV_FORMAT_FLAG);
                                true
                            });
                        }
                        ("loop-file", MPV_FORMAT_STRING) => {
                            state.loop_file.replace(data!(prop, String));
                        }
                        ("loop-playlist", MPV_FORMAT_STRING) => {
                            state.loop_playlist.replace(data!(prop, String));
                        }
                        ("fullscreen", MPV_FORMAT_FLAG) => {
                            root_changed.insert("Fullscreen", data!(prop, bool).into());
                        }
                        ("seekable", MPV_FORMAT_FLAG) => {
                            player_changed.insert("CanSeek", data!(prop, bool).into());
                        }
                        ("idle-active", MPV_FORMAT_FLAG) => {
                            state.idle_active.replace(data!(prop, bool));
                        }
                        ("eof-reached", MPV_FORMAT_FLAG) => {
                            state.eof_reached.replace(data!(prop, bool));
                        }
                        ("pause", MPV_FORMAT_FLAG) => {
                            state.pause.replace(data!(prop, bool));
                        }
                        ("shuffle", MPV_FORMAT_FLAG) => {
                            player_changed.insert("Shuffle", data!(prop, bool).into());
                        }
                        ("speed", MPV_FORMAT_DOUBLE) => {
                            player_changed.insert("Rate", data!(prop, f64).into());
                        }
                        ("volume", MPV_FORMAT_DOUBLE) => {
                            player_changed.insert("Volume", (data!(prop, f64) / 100.0).into());
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        signal_changed(
            ctx,
            state,
            (&root_ctxt, &mut root_changed),
            (&player_ctxt, &mut player_changed),
        )
        .for_each(|err| err.unwrap_or_else(elog));
    }
}

#[derive(Clone, Default)]
struct State {
    idle_active: Option<bool>,
    keep_open: Option<bool>,
    eof_reached: Option<bool>,
    pause: Option<bool>,
    loop_file: Option<String>,
    loop_playlist: Option<String>,
    metadata: bool,
}

impl State {
    const fn playback_status(&self) -> bool {
        self.idle_active.is_some()
            | self.keep_open.is_some()
            | self.eof_reached.is_some()
            | self.pause.is_some()
    }
    const fn loop_status(&self) -> bool {
        self.loop_file.is_some() | self.loop_playlist.is_some()
    }
}

fn signal_changed(
    ctx: Handle,
    state: State,
    (root_ctxt, root_changed): (
        &SignalContext<'_>,
        &mut HashMap<&'static str, Value<'static>>,
    ),
    (player_ctxt, player_changed): (
        &SignalContext<'_>,
        &mut HashMap<&'static str, Value<'static>>,
    ),
) -> impl Iterator<Item = zbus::Result<()>> {
    if state.playback_status() {
        if let Ok(value) =
            mpris2::playback_status_from(ctx, state.idle_active, state.eof_reached, state.pause)
        {
            player_changed.insert("PlaybackStatus", value.into());
        }
    }
    if state.loop_status() {
        if let Ok(value) = mpris2::loop_status_from(ctx, state.loop_file, state.loop_playlist) {
            player_changed.insert("LoopStatus", value.into());
        }
    }
    if state.metadata {
        if let Ok(value) = block_on(mpris2::metadata(ctx)) {
            player_changed.insert("Metadata", value.into());
        }
    }
    [
        (!root_changed.is_empty()).then(|| {
            let root = properties_changed(root_ctxt, mpris2::Root::name(), root_changed);
            root_changed.clear();
            root
        }),
        (!player_changed.is_empty()).then(|| {
            let player = properties_changed(player_ctxt, mpris2::Player::name(), player_changed);
            player_changed.clear();
            player
        }),
    ]
    .into_iter()
    .flatten()
}
