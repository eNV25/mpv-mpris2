use std::io::Error;
use std::os::fd::{FromRawFd, OwnedFd, RawFd};

pub(crate) fn mpv_ipc_fd() -> anyhow::Result<OwnedFd> {
    let mut args = pico_args::Arguments::from_env();
    let fd: RawFd = args.value_from_str("--mpv-ipc-fd")?;
    // Check the fd is open, and stop child processes (ffmpeg) inheriting it.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0 {
        Err(Error::last_os_error())?;
    }
    // SAFETY: the fd is open, and mpv hands it to this process to own.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}
