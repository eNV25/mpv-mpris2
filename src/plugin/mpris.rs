use crate::{
    common::{FutureSyncExt, time_as_secs, time_from_secs},
    mpv::{ListCommand, LoadFlags, NamedCommand, SeekFlags, SeekMode},
};
use mpris_server::{
    LoopStatus, Metadata, PlaybackRate, PlaybackStatus, PlayerInterface, RootInterface, Time,
    TrackId, Volume,
};
use smol::lock::OnceCell;
use zbus::fdo;

impl RootInterface for super::Player {
    async fn raise(&self) -> fdo::Result<()> {
        Ok(())
    }

    async fn quit(&self) -> fdo::Result<()> {
        let cmd = NamedCommand::Quit { code: None };
        Ok(self.mpv.run_command(cmd).sync().await?)
    }

    async fn can_quit(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn fullscreen(&self) -> fdo::Result<bool> {
        Ok(self.state.read().await.fullscreen)
    }

    async fn set_fullscreen(&self, value: bool) -> zbus::Result<()> {
        Ok(self.mpv.set_property("fullscreen", value).sync().await?)
    }

    async fn can_set_fullscreen(&self) -> fdo::Result<bool> {
        Ok(true)
    }

    async fn can_raise(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn has_track_list(&self) -> fdo::Result<bool> {
        Ok(false)
    }

    async fn identity(&self) -> fdo::Result<String> {
        Ok("mpv".into())
    }

    async fn desktop_entry(&self) -> fdo::Result<String> {
        Ok("mpv Media Player".into())
    }

    async fn supported_uri_schemes(&self) -> fdo::Result<Vec<String>> {
        static SUPPORTED_URI_SCHEMES: OnceCell<Vec<String>> = OnceCell::new();
        Ok(SUPPORTED_URI_SCHEMES
            .get_or_init(|| async {
                self.mpv
                    .get_property::<String>("protocol-list")
                    .sync()
                    .await
                    .ok()
                    .iter()
                    .flat_map(|s| s.split(','))
                    .map(str::to_owned)
                    .collect()
            })
            .await
            .clone())
    }

    async fn supported_mime_types(&self) -> fdo::Result<Vec<String>> {
        #[crabtime::function]
        fn define_mpv_mime_types(pattern!($name:ident): _) {
            let value = format!(
                "{:?}",
                std::env::var("XDG_DATA_DIRS")
                    .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_owned())
                    .split(':')
                    .filter_map(|dir| {
                        let path = std::path::Path::new(dir).join("applications/mpv.desktop");
                        std::fs::read_to_string(path).ok()
                    })
                    .find_map(|content| {
                        content.lines().find_map(|line| {
                            line.strip_prefix("MimeType=").map(|v| {
                                v.split_terminator(';')
                                    .map(str::to_owned)
                                    .collect::<Vec<_>>()
                            })
                        })
                    })
                    .expect(
                        "Failed to find mpv.desktop at build time. Ensure mpv is installed, or set XDG_DATA_DIRS appropriately."
                    )
            );
            _ = value;
            crabtime::output! {
                const $name: &[&str] = &{{value}};
            }
        }
        define_mpv_mime_types!(MPV_MIME_TYPES);
        Ok(MPV_MIME_TYPES.iter().map(|&x| x.to_owned()).collect())
    }
}

impl PlayerInterface for super::Player {
    async fn next(&self) -> fdo::Result<()> {
        let cmd = NamedCommand::PlaylistNext { flags: None };
        Ok(self.mpv.run_command(cmd).sync().await?)
    }

    async fn previous(&self) -> fdo::Result<()> {
        let cmd = NamedCommand::PlaylistPrev { flags: None };
        Ok(self.mpv.run_command(cmd).sync().await?)
    }

    async fn pause(&self) -> fdo::Result<()> {
        Ok(self.mpv.set_property("pause", true).sync().await?)
    }

    async fn play_pause(&self) -> fdo::Result<()> {
        let cmd = ListCommand::Cycle("pause", None);
        Ok(self.mpv.run_command(cmd).sync().await?)
    }

    async fn stop(&self) -> fdo::Result<()> {
        self.pause().await?;
        let cmd = NamedCommand::Seek {
            target: 0.0,
            flags: Some(SeekFlags(Some(SeekMode::Absolute), None)),
        };
        Ok(self.mpv.run_command(cmd).sync().await?)
    }

    async fn play(&self) -> fdo::Result<()> {
        Ok(self.mpv.set_property("pause", false).sync().await?)
    }

    async fn seek(&self, offset: Time) -> fdo::Result<()> {
        let offset = time_as_secs(offset);
        let cmd = NamedCommand::Seek {
            target: offset,
            flags: Some(SeekFlags(Some(SeekMode::Relative), None)),
        };
        Ok(self.mpv.run_command(cmd).sync().await?)
    }

    async fn set_position(&self, track_id: TrackId, position: Time) -> fdo::Result<()> {
        let id = track_id
            .strip_prefix("/io/mpv/playlist_entry_id/")
            .map(str::parse)
            .transpose()
            .map_err(|e| fdo::Error::InvalidArgs(format!("Invalid track ID: {e}")))?;
        if id.is_some() && id == self.state.read().await.playlist_entry_id {
            let value = time_as_secs(position);
            self.mpv.set_property("playback-time", value).sync().await?;
            return Ok(());
        }
        Err(fdo::Error::InvalidArgs("Invalid track ID".into()))
    }

    async fn open_uri(&self, uri: String) -> fdo::Result<()> {
        let cmd = NamedCommand::Loadfile {
            url: uri,
            flags: Some(LoadFlags::Replace),
            index: None,
            options: None,
        };
        Ok(self.mpv.run_command(cmd).sync().await?)
    }

    async fn playback_status(&self) -> fdo::Result<PlaybackStatus> {
        Ok(self.state.read().await.playback_status())
    }

    async fn loop_status(&self) -> fdo::Result<LoopStatus> {
        Ok(self.state.read().await.loop_status())
    }

    async fn set_loop_status(&self, loop_status: LoopStatus) -> zbus::Result<()> {
        self.mpv
            .set_property(
                "loop-file",
                match loop_status {
                    LoopStatus::Track => "inf",
                    _ => "no",
                },
            )
            .sync()
            .await?;
        self.mpv
            .set_property(
                "loop-playlist",
                match loop_status {
                    LoopStatus::Playlist => "inf",
                    _ => "no",
                },
            )
            .sync()
            .await?;
        Ok(())
    }

    async fn rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(self.state.read().await.speed)
    }

    async fn set_rate(&self, rate: PlaybackRate) -> zbus::Result<()> {
        Ok(self.mpv.set_property("speed", rate).sync().await?)
    }

    async fn shuffle(&self) -> fdo::Result<bool> {
        Ok(self.state.read().await.shuffle)
    }

    async fn set_shuffle(&self, shuffle: bool) -> zbus::Result<()> {
        Ok(self.mpv.set_property("shuffle", shuffle).sync().await?)
    }

    async fn metadata(&self) -> fdo::Result<Metadata> {
        let state = self.state.read().await;
        state.metadata().map_err(fdo::Error::Failed)
    }

    async fn volume(&self) -> fdo::Result<Volume> {
        Ok(self.state.read().await.volume())
    }

    async fn set_volume(&self, volume: Volume) -> zbus::Result<()> {
        let volume = volume * 100.0;
        Ok(self.mpv.set_property("volume", volume).sync().await?)
    }

    async fn position(&self) -> fdo::Result<Time> {
        let position = self.mpv.get_property("playback-time").sync().await?;
        Ok(time_from_secs(position))
    }

    async fn minimum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(0.01)
    }

    async fn maximum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(100.0)
    }

    async fn can_go_next(&self) -> fdo::Result<bool> {
        Ok(self.state.read().await.playlist_has_next())
    }

    async fn can_go_previous(&self) -> fdo::Result<bool> {
        Ok(self.state.read().await.playlist_has_previous())
    }

    async fn can_play(&self) -> fdo::Result<bool> {
        Ok(self.state.read().await.playlist_entry_id.is_some())
    }

    async fn can_pause(&self) -> fdo::Result<bool> {
        Ok(self.state.read().await.playlist_entry_id.is_some())
    }

    async fn can_seek(&self) -> fdo::Result<bool> {
        Ok(self.state.read().await.seekable)
    }

    async fn can_control(&self) -> fdo::Result<bool> {
        Ok(true)
    }
}
