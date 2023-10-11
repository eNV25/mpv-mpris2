#![warn(clippy::suspicious)]
#![warn(clippy::complexity)]
#![warn(clippy::perf)]
#![warn(clippy::pedantic)]
#![allow(special_module_name)]

use std::process;
use std::{collections::HashMap, ffi::c_int, iter};

use zbus::blocking::ConnectionBuilder;
use zbus::{zvariant::Value, Interface, SignalContext};

#[allow(clippy::wildcard_imports)]
use crate::ffi::*;
#[allow(clippy::wildcard_imports)]
use crate::lib::*;

mod ffi;
mod lib;
mod macros;
mod mpris2;

macro_rules! cstr {
    ($s:expr) => {
        std::ffi::CStr::from_ptr($s).to_str().unwrap_or_default()
    };
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn mpv_open_cplugin(ctx: *mut mpv_handle) -> c_int {
    if ctx.is_null() {
        return 1;
    }
    let script = unsafe { cstr!(mpv_client_name(ctx)) };
    match plugin(Handle(ctx), script) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("[{script}] {err:?}");
            1
        }
    }
}

macro_rules! data {
    ($source:expr, bool) => {
        data!($source, std::ffi::c_int) != 0
    };
    ($source:expr, &str) => {
        cstr!(*$source.data.cast())
    };
    ($source:expr, $type:ty) => {
        *$source.data.cast::<$type>()
    };
}

fn plugin(ctx: Handle, name: &str) -> anyhow::Result<()> {
    let path = "/org/mpris/MediaPlayer2";
    let pid = process::id();
    let connection = ConnectionBuilder::session()?
        .name(format!("org.mpris.MediaPlayer2.mpv.instance{pid}"))?
        .serve_at(path, crate::mpris2::Root(ctx))?
        .serve_at(path, crate::mpris2::Player(ctx))?
        .build()?;
    let connection = SignalContext::from_parts(connection.into_inner(), path.try_into()?);

    observe_properties(ctx);

    let elog = |err| eprintln!("[{name}] {err:?}");

    let mut keep_open = false;
    let mut seeking = false;
    let mut root = Vec::new();
    let mut player = Vec::new();
    loop {
        let mut state = scopeguard::guard(
            (State::default(), &mut root, &mut player),
            |(state, root, player)| {
                signal_changed(ctx, state, &connection, root, player)
                    .for_each(|err| err.unwrap_or_else(elog));
            },
        );
        let (state, &mut ref mut root_changed, &mut ref mut player_changed) = &mut *state;

        for ev in iter::once(-1.0)
            .chain(iter::repeat(0.0))
            .map(|timeout| unsafe { *mpv_wait_event(ctx.into(), timeout) })
        {
            match ev.event_id {
                MPV_EVENT_NONE => break,
                MPV_EVENT_SHUTDOWN => return Ok(()),
                MPV_EVENT_SEEK => seeking = true,
                MPV_EVENT_PLAYBACK_RESTART if seeking => {
                    seeking = false;
                    if let Ok(position) = get!(ctx, "playback-time", f64) {
                        seeked(&connection, mpris2::time_from_secs(position)).unwrap_or_else(elog);
                    }
                }
                MPV_EVENT_PROPERTY_CHANGE => {
                    let prop = unsafe { data!(ev, mpv_event_property) };
                    let name = unsafe { cstr!(prop.name) };
                    match (name, prop.format) {
                        ("media-title" | "metadata" | "duration", _) => {
                            state.metadata = true;
                        }
                        ("keep-open", MPV_FORMAT_STRING) => {
                            let value = unsafe { cstr!(*prop.data.cast()) } != "no";
                            keep_open = value;
                            state.keep_open = Some(value);
                        }
                        ("loop-file", MPV_FORMAT_STRING) => {
                            state.loop_file = Some(unsafe { data!(prop, &str) } != "no");
                        }
                        ("loop-playlist", MPV_FORMAT_STRING) => {
                            state.loop_playlist = Some(unsafe { data!(prop, &str) } != "no");
                        }
                        ("fullscreen", MPV_FORMAT_FLAG) => {
                            root_changed.push(("Fullscreen", unsafe { data!(prop, bool) }.into()));
                        }
                        ("seekable", MPV_FORMAT_FLAG) => {
                            player_changed.push(("CanSeek", unsafe { data!(prop, bool) }.into()));
                        }
                        ("idle-active", MPV_FORMAT_FLAG) => {
                            state.idle_active = Some(unsafe { data!(prop, bool) });
                        }
                        ("eof-reached", MPV_FORMAT_FLAG) if keep_open => {
                            state.eof_reached = Some(unsafe { data!(prop, bool) });
                        }
                        ("pause", MPV_FORMAT_FLAG) => {
                            state.pause = Some(unsafe { data!(prop, bool) });
                        }
                        ("shuffle", MPV_FORMAT_FLAG) => {
                            player_changed.push(("Shuffle", unsafe { data!(prop, bool) }.into()));
                        }
                        ("speed", MPV_FORMAT_DOUBLE) => {
                            player_changed.push(("Rate", unsafe { data!(prop, f64) }.into()));
                        }
                        ("volume", MPV_FORMAT_DOUBLE) => {
                            player_changed
                                .push(("Volume", (unsafe { data!(prop, f64) } / 100.0).into()));
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}

// These properties and those handled in the main loop must be kept in sync with those
// mentioned in the interface implementations.
// It's a bit of a pain in the ass but there's no other way.
fn observe_properties(ctx: Handle) {
    observe!(ctx, "media-title", "metadata", "duration");
    observe!(
        ctx,
        MPV_FORMAT_FLAG,
        "fullscreen",
        "seekable",
        "idle-active",
        "eof-reached",
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
}

#[derive(Clone, Default)]
struct State {
    idle_active: Option<bool>,
    keep_open: Option<bool>,
    eof_reached: Option<bool>,
    pause: Option<bool>,
    loop_file: Option<bool>,
    loop_playlist: Option<bool>,
    metadata: bool,
}

impl State {
    fn playback_status(&mut self, ctx: Handle) -> Option<Value<'static>> {
        if self.idle_active.is_some()
            | self.keep_open.is_some()
            | self.eof_reached.is_some()
            | self.pause.is_some()
        {
            self.keep_open.take();
            mpris2::playback_status_from(
                ctx,
                self.idle_active.take(),
                self.eof_reached.take(),
                self.pause.take(),
            )
            .ok()
            .map(Into::into)
        } else {
            None
        }
    }
    fn loop_status(&mut self, ctx: Handle) -> Option<Value<'static>> {
        if self.loop_file.is_some() | self.loop_playlist.is_some() {
            mpris2::loop_status_from(ctx, self.loop_file.take(), self.loop_playlist.take())
                .ok()
                .map(Into::into)
        } else {
            None
        }
    }
    fn metadata(&mut self, ctx: Handle) -> Option<Value<'static>> {
        if self.metadata {
            mpris2::metadata(ctx).ok().map(Into::into)
        } else {
            None
        }
    }
}

fn signal_changed(
    ctx: Handle,
    mut state: State,
    ctxt: &SignalContext<'_>,
    root: &mut Vec<(&str, Value<'_>)>,
    player: &mut Vec<(&str, Value<'_>)>,
) -> impl Iterator<Item = zbus::Result<()>> {
    let playback_status = state.playback_status(ctx);
    let loop_status = state.loop_status(ctx);
    let metadata = state.metadata(ctx);

    let (root, player) = (root.drain(..), player.drain(..));
    let root: HashMap<_, _> = root.as_slice().iter().map(|(k, v)| (*k, v)).collect();
    let mut player: HashMap<_, _> = player.as_slice().iter().map(|(k, v)| (*k, v)).collect();
    if let Some(ref value) = playback_status {
        player.insert("PlaybackStatus", value);
    }
    if let Some(ref value) = loop_status {
        player.insert("LoopStatus", value);
    }
    if let Some(ref value) = metadata {
        player.insert("Metadata", value);
    }

    let root = (!root.is_empty()).then(|| properties_changed(ctxt, mpris2::Root::name(), &root));
    let player =
        (!player.is_empty()).then(|| properties_changed(ctxt, mpris2::Player::name(), &player));
    root.into_iter().chain(player)
}
