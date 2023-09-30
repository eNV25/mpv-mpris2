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
        Ok(_) => 0,
        Err(err) => {
            eprintln!("[{script}] {err:?}");
            1
        }
    }
}

#[allow(clippy::too_many_lines)]
fn plugin(ctx: Handle, name: &str) -> anyhow::Result<()> {
    pub const OBJ_PATH: &str = "/org/mpris/MediaPlayer2";
    let pid = process::id();
    let connection = ConnectionBuilder::session()?
        .name(format!("org.mpris.MediaPlayer2.mpv.instance{pid}"))?
        .serve_at(OBJ_PATH, crate::mpris2::Root(ctx))?
        .serve_at(OBJ_PATH, crate::mpris2::Player(ctx))?
        .build()?;
    let object_server = connection.object_server();
    let root = object_server.interface::<_, mpris2::Root>(OBJ_PATH)?;
    let player = object_server.interface::<_, mpris2::Player>(OBJ_PATH)?;

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

    let elog = |err| eprintln!("[{name}] {err:?}");

    let mut keep_open = false;
    let mut seeking = false;
    let mut root_changed = HashMap::new();
    let mut player_changed = HashMap::new();
    loop {
        let mut state = scopeguard::guard(
            (State::default(), &mut root_changed, &mut player_changed),
            |(state, root_changed, player_changed)| {
                let root_changed_refs = root_changed.iter().map(|(&k, v)| (k, v)).collect();
                let player_changed_refs = player_changed.iter().map(|(&k, v)| (k, v)).collect();
                signal_changed(
                    ctx,
                    state,
                    (root.signal_context(), root_changed_refs),
                    (player.signal_context(), player_changed_refs),
                )
                .for_each(|err| err.unwrap_or_else(elog));
                root_changed.clear();
                player_changed.clear();
            },
        );
        let (state, root_changed, player_changed) = &mut *state;

        for ev in iter::once(-1.0)
            .chain(iter::repeat(0.0))
            .map(|timeout| unsafe { *mpv_wait_event(ctx.into(), timeout) })
        {
            macro_rules! data {
                ($source:expr, bool) => {
                    data!($source, std::ffi::c_int) != 0
                };
                ($source:expr, String) => {
                    cstr!(*$source.data.cast()).to_owned()
                };
                ($source:expr, $type:ty) => {
                    *$source.data.cast::<$type>()
                };
            }
            match ev.event_id {
                MPV_EVENT_NONE => break,
                MPV_EVENT_SHUTDOWN => return Ok(()),
                MPV_EVENT_SEEK => seeking = true,
                MPV_EVENT_PLAYBACK_RESTART if seeking => {
                    seeking = false;
                    if let Ok(position) = get!(ctx, "playback-time", f64) {
                        seeked(player.signal_context(), mpris2::time_from_secs(position))
                            .unwrap_or_else(elog);
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
                            state.keep_open.replace(value);
                        }
                        ("loop-file", MPV_FORMAT_STRING) => {
                            state.loop_file.replace(unsafe { data!(prop, String) });
                        }
                        ("loop-playlist", MPV_FORMAT_STRING) => {
                            state.loop_playlist.replace(unsafe { data!(prop, String) });
                        }
                        ("fullscreen", MPV_FORMAT_FLAG) => {
                            root_changed.insert("Fullscreen", unsafe { data!(prop, bool) }.into());
                        }
                        ("seekable", MPV_FORMAT_FLAG) => {
                            player_changed.insert("CanSeek", unsafe { data!(prop, bool) }.into());
                        }
                        ("idle-active", MPV_FORMAT_FLAG) => {
                            state.idle_active.replace(unsafe { data!(prop, bool) });
                        }
                        ("eof-reached", MPV_FORMAT_FLAG) if keep_open => {
                            state.eof_reached.replace(unsafe { data!(prop, bool) });
                        }
                        ("pause", MPV_FORMAT_FLAG) => {
                            state.pause.replace(unsafe { data!(prop, bool) });
                        }
                        ("shuffle", MPV_FORMAT_FLAG) => {
                            player_changed.insert("Shuffle", unsafe { data!(prop, bool) }.into());
                        }
                        ("speed", MPV_FORMAT_DOUBLE) => {
                            player_changed.insert("Rate", unsafe { data!(prop, f64) }.into());
                        }
                        ("volume", MPV_FORMAT_DOUBLE) => {
                            player_changed
                                .insert("Volume", (unsafe { data!(prop, f64) } / 100.0).into());
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
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
    (root_ctxt, root_changed): (&SignalContext<'_>, HashMap<&'static str, &Value<'static>>),
    (player_ctxt, mut player_changed): (&SignalContext<'_>, HashMap<&'static str, &Value<'static>>),
) -> impl Iterator<Item = zbus::Result<()>> {
    let playback_status = state.playback_status(ctx);
    let loop_status = state.loop_status(ctx);
    let metadata = state.metadata(ctx);
    if let Some(ref value) = playback_status {
        player_changed.insert("PlaybackStatus", value);
    }
    if let Some(ref value) = loop_status {
        player_changed.insert("LoopStatus", value);
    }
    if let Some(ref value) = metadata {
        player_changed.insert("Metadata", value);
    }
    let root = (!root_changed.is_empty())
        .then(|| properties_changed(root_ctxt, mpris2::Root::name(), &root_changed));
    let player = (!player_changed.is_empty())
        .then(|| properties_changed(player_ctxt, mpris2::Player::name(), &player_changed));
    root.into_iter().chain(player)
}
