#![allow(clippy::not_unsafe_ptr_arg_deref)] // mpv_wait_event

use std::{
    ffi::{c_int, CStr},
    process,
};

use mpv::capi::{mpv_event_id::*, *};

pub(crate) mod mpris;
pub(crate) mod mpv;

#[no_mangle]
pub extern "C" fn mpv_open_cplugin(ctx: *mut mpv_handle) -> c_int {
    if ctx.is_null() {
        return 1;
    }
    let pid = process::id();
    let connection = smol::block_on(async {
        let connection = zbus::ConnectionBuilder::session()?
            .name(format!("org.mpris.MediaPlayer2.mpv.instance{pid}"))?
            .serve_at("/org/mpris/MediaPlayer2", mpris::RootProxy::from(ctx))?
            .serve_at("/org/mpris/MediaPlayer2", mpris::PlayerProxy::from(ctx))?
            .build()
            .await?;
        zbus::Result::Ok(connection)
    })
    .expect("dbus session connection and server setup");

    let root = smol::block_on(
        connection
            .object_server()
            .interface::<_, mpris::RootProxy>("/org/mpris/MediaPlayer2"),
    )
    .expect("MediaPlayer2 interface reference");
    let player = smol::block_on(
        connection
            .object_server()
            .interface::<_, mpris::PlayerProxy>("/org/mpris/MediaPlayer2"),
    )
    .expect("MediaPlayer2.Player interface reference");

    // These properties and those handled in the main loop
    // must be kept in sync with the implementations in the
    // dbus interface implementations.
    // It's a bit of a pain in the ass but there's no other way.
    mpv::observe_properties!(
        ctx,
        "seekable\0",
        "idle-active\0",
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

    let mut mp_seeking = false;
    loop {
        let ev = unsafe { mpv_wait_event(ctx, -1.0).as_ref().unwrap_unchecked() };
        if ev.reply_userdata != u64::default() && ev.reply_userdata != mpv::REPLY_USERDATA {
            continue;
        }
        match ev.event_id {
            MPV_EVENT_SHUTDOWN => return 0,
            MPV_EVENT_SEEK => {
                mp_seeking = true;
            }
            MPV_EVENT_PLAYBACK_RESTART if mp_seeking => smol::block_on(async {
                mp_seeking = false;
                _ = mpris::PlayerProxy::seeked(
                    player.signal_context(),
                    (mpv::get_property_float!(ctx, "playback-time\0") * 1E6) as i64,
                )
                .await;
            }),
            MPV_EVENT_PROPERTY_CHANGE => smol::block_on(async {
                let data = unsafe {
                    (ev.data as *const mpv_event_property)
                        .as_ref()
                        .unwrap_unchecked()
                };
                let prop = unsafe { CStr::from_ptr(data.name) }
                    .to_str()
                    .unwrap_or_default();
                macro_rules! changed {
                    ($iface:expr, $method:ident) => {
                        $iface.get().await.$method($iface.signal_context()).await
                    };
                }
                _ = match prop {
                    "seekable" => changed!(player, can_seek_changed),
                    "idle-active" | "eof-reached" | "pause" => {
                        changed!(player, playback_status_changed)
                    }
                    "loop-file" | "loop-playlist" => changed!(player, loop_status_changed),
                    "speed" => changed!(player, rate_changed),
                    "shuffle" => changed!(player, shuffle_changed),
                    "metadata" => changed!(player, metadata_changed),
                    "volume" => changed!(player, volume_changed),
                    "fullscreen" => changed!(root, fullscreen_changed),
                    _ => Ok(()),
                };
            }),
            _ => {}
        }
    }
}
