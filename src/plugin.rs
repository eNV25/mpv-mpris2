#![warn(clippy::suspicious)]
#![warn(clippy::complexity)]
#![warn(clippy::perf)]
#![warn(clippy::pedantic)]

use std::{collections::HashMap, ffi::c_int, iter, process, thread};

use zbus::zvariant;

#[allow(clippy::wildcard_imports)]
use crate::ffi::*;
#[allow(clippy::wildcard_imports)]
use crate::llb::*;

mod ffi;
mod llb;
mod macros;
mod mpris2;

macro_rules! cstr {
    ($s:expr) => {
        std::ffi::CStr::from_ptr($s).to_str().unwrap_or_default()
    };
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn mpv_open_cplugin(mpv: MPVHandle) -> c_int {
    if mpv.0.is_null() {
        return 1;
    }

    let name = unsafe { cstr!(mpv_client_name(mpv.into())) };

    // try blocks are not stable yet, ugh
    let ctxt = match (|| {
        use zbus::names::WellKnownName;
        use zvariant::ObjectPath;
        const PATH_STR: &str = "/org/mpris/MediaPlayer2";
        const PATH: ObjectPath<'_> = ObjectPath::from_static_str_unchecked(PATH_STR);
        let pid = process::id();
        let name = format!("org.mpris.MediaPlayer2.mpv.instance{pid}");
        let connection = zbus::ConnectionBuilder::session()?
            .name(WellKnownName::from_string_unchecked(name))?
            .serve_at(PATH, crate::mpris2::Root(mpv))?
            .serve_at(PATH, crate::mpris2::Player(mpv))?
            .internal_executor(false)
            .build()
            .block()?;
        let executor = connection.executor().clone();
        thread::Builder::new()
            .name("mpv/mpris/zbus".into())
            .spawn(move || {
                async move {
                    while !executor.is_empty() {
                        executor.tick().await;
                    }
                }
                .block();
            })?;
        zbus::Result::Ok(zbus::SignalContext::from_parts(connection, PATH))
    })() {
        Ok(ctxt) => ctxt,
        Err(err) => {
            eprintln!("[{name}]: {err}");
            return 1;
        }
    };

    observe_properties(mpv);

    main_loop(mpv, &ctxt, name);

    0
}

fn main_loop(mpv: MPVHandle, ctxt: &zbus::SignalContext, name: &str) {
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

    let elog = |err| eprintln!("[{name}] {err:?}");

    let mut keep_open = false;
    let mut seeking = false;
    let mut root = Vec::new();
    let mut player = Vec::new();
    loop {
        let mut state = scopeguard::guard(
            (State::default(), &mut root, &mut player),
            |(state, root, player)| {
                signal_changed(mpv, state, ctxt, root, player)
                    .for_each(|err| err.unwrap_or_else(elog));
            },
        );
        let (state, &mut ref mut root_changed, &mut ref mut player_changed) = &mut *state;

        for ev in iter::once(-1.0)
            .chain(iter::repeat(0.0))
            .map(|timeout| unsafe { *mpv_wait_event(mpv.into(), timeout) })
        {
            match ev.event_id {
                MPV_EVENT_NONE => break,
                MPV_EVENT_SHUTDOWN => return,
                MPV_EVENT_SEEK => seeking = true,
                MPV_EVENT_PLAYBACK_RESTART if seeking => {
                    seeking = false;
                    if let Ok(position) = get!(mpv, "playback-time", f64) {
                        seeked(ctxt, mpris2::time_from_secs(position)).unwrap_or_else(elog);
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
                            keep_open = unsafe { data!(prop, &str) } != "no";
                            state.keep_open = Some(keep_open);
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
fn observe_properties(mpv: MPVHandle) {
    observe!(mpv, "media-title", "metadata", "duration");
    observe!(
        mpv,
        MPV_FORMAT_FLAG,
        "fullscreen",
        "seekable",
        "idle-active",
        "eof-reached",
        "pause",
        "shuffle",
    );
    observe!(
        mpv,
        MPV_FORMAT_STRING,
        "keep-open",
        "loop-file",
        "loop-playlist",
    );
    observe!(mpv, MPV_FORMAT_DOUBLE, "speed", "volume");
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
    fn playback_status(&mut self, mpv: MPVHandle) -> Option<zvariant::Value<'static>> {
        if self.idle_active.is_some()
            | self.keep_open.is_some()
            | self.eof_reached.is_some()
            | self.pause.is_some()
        {
            self.keep_open.take();
            mpris2::playback_status_from(
                mpv,
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
    fn loop_status(&mut self, mpv: MPVHandle) -> Option<zvariant::Value<'static>> {
        if self.loop_file.is_some() | self.loop_playlist.is_some() {
            mpris2::loop_status_from(mpv, self.loop_file.take(), self.loop_playlist.take())
                .ok()
                .map(Into::into)
        } else {
            None
        }
    }
    fn metadata(&mut self, mpv: MPVHandle) -> Option<zvariant::Value<'static>> {
        if self.metadata {
            mpris2::metadata(mpv).ok().map(Into::into)
        } else {
            None
        }
    }
}

fn signal_changed(
    mpv: MPVHandle,
    mut state: State,
    ctxt: &zbus::SignalContext<'_>,
    root: &mut Vec<(&str, zvariant::Value<'_>)>,
    player: &mut Vec<(&str, zvariant::Value<'_>)>,
) -> impl Iterator<Item = zbus::Result<()>> {
    let playback_status = state.playback_status(mpv);
    let loop_status = state.loop_status(mpv);
    let metadata = state.metadata(mpv);

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

    let root = (!root.is_empty()).then(|| properties_changed::<mpris2::Root>(ctxt, &root));
    let player = (!player.is_empty()).then(|| properties_changed::<mpris2::Player>(ctxt, &player));
    root.into_iter().chain(player)
}
