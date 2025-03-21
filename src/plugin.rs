use std::{collections::HashMap, ffi::c_int, iter, process, thread, vec};

use zbus::zvariant;

#[allow(clippy::wildcard_imports)]
use crate::ffi::*;
#[allow(clippy::wildcard_imports)]
use crate::llb::*;

mod ffi;
mod llb;
mod macros;
mod mpris2;

macro_rules! strc {
    ($s:expr) => {
        std::ffi::CStr::from_ptr($s).to_str().unwrap_or_default()
    };
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn mpv_open_cplugin(mpv: *mut mpv_handle) -> c_int {
    if mpv.is_null() {
        return 1;
    }

    let name = strc!(mpv_client_name(mpv));
    let mpv = MPVHandle(mpv);

    match init(mpv).as_ref() {
        Ok(ctxt) => {
            register(mpv);
            do_loop(mpv, ctxt, name);
            0
        }
        Err(err) => {
            eprintln!("[{name}]: {err}");
            1
        }
    }
}

fn init(mpv: MPVHandle) -> zbus::Result<zbus::object_server::SignalEmitter<'static>> {
    use zbus::names::WellKnownName;
    use zvariant::ObjectPath;
    const PATH_STR: &str = "/org/mpris/MediaPlayer2";
    const PATH: ObjectPath<'_> = ObjectPath::from_static_str_unchecked(PATH_STR);
    let pid = process::id();
    let name = format!("org.mpris.MediaPlayer2.mpv.instance{pid}");
    let connection = zbus::connection::Builder::session()?
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
    let connection = zbus::object_server::SignalEmitter::from_parts(connection, PATH);
    Ok(connection)
}

// These properties and those handled in the main loop must be kept in sync with those
// mentioned in the interface implementations.
// It's a bit of a pain in the ass but there's no other way.
fn register(mpv: MPVHandle) {
    observe!(mpv, c"media-title", c"metadata", c"duration");
    observe!(
        mpv,
        MPV_FORMAT_STRING,
        c"keep-open",
        c"loop-file",
        c"loop-playlist",
    );
    observe!(
        mpv,
        MPV_FORMAT_FLAG,
        c"fullscreen",
        c"seekable",
        c"idle-active",
        c"eof-reached",
        c"pause",
        c"shuffle",
    );
    observe!(mpv, MPV_FORMAT_DOUBLE, c"speed", c"volume");
}

fn do_loop(mpv: MPVHandle, ctxt: &zbus::object_server::SignalEmitter, name: &str) {
    macro_rules! data {
        ($source:expr, bool) => {
            data!($source, std::ffi::c_int) != 0
        };
        ($source:expr, &str) => {
            strc!(*$source.data.cast())
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
                signal_changed(mpv, state, ctxt, root.drain(..), player.drain(..))
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
                    if let Ok(position) = get!(mpv, c"playback-time", f64) {
                        seeked(ctxt, mpris2::time_from_secs(position)).unwrap_or_else(elog);
                    }
                }
                MPV_EVENT_PROPERTY_CHANGE => {
                    let prop = unsafe { data!(ev, mpv_event_property) };
                    let name = unsafe { strc!(prop.name) };
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
        } else {
            None
        }
        .map(Into::into)
    }
    fn loop_status(&mut self, mpv: MPVHandle) -> Option<zvariant::Value<'static>> {
        if self.loop_file.is_some() | self.loop_playlist.is_some() {
            Some(mpris2::loop_status_from(
                mpv,
                self.loop_file.take(),
                self.loop_playlist.take(),
            ))
        } else {
            None
        }
        .map(Into::into)
    }
    fn metadata(&mut self, mpv: MPVHandle) -> Option<zvariant::Value<'static>> {
        if self.metadata {
            Some(mpris2::metadata(mpv))
        } else {
            None
        }
        .map(Into::into)
    }
}

fn signal_changed(
    mpv: MPVHandle,
    mut state: State,
    emitter: &zbus::object_server::SignalEmitter<'_>,
    root: vec::Drain<(&str, zvariant::Value<'_>)>,
    player: vec::Drain<(&str, zvariant::Value<'_>)>,
) -> impl Iterator<Item = zbus::Result<()>> {
    let root: HashMap<_, _> = root.collect();
    let mut player: HashMap<_, _> = player.collect();

    state
        .playback_status(mpv)
        .and_then(|v| player.insert("PlaybackStatus", v));
    state
        .loop_status(mpv)
        .and_then(|v| player.insert("LoopStatus", v));
    state
        .metadata(mpv)
        .and_then(|v| player.insert("Metadata", v));

    [
        properties_changed::<mpris2::Root>(emitter, root),
        properties_changed::<mpris2::Player>(emitter, player),
    ]
    .into_iter()
}
