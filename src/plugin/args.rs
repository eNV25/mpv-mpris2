use std::io::Error;
use std::os::fd::{FromRawFd, OwnedFd};

pub(crate) fn mpv_ipc_fd() -> anyhow::Result<OwnedFd> {
    let mut args = pico_args::Arguments::from_env();
    let fd = args.value_from_str("--mpv-ipc-fd")?;
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFD);
        if flags < 0 {
            Err(Error::last_os_error())?;
        }
        Ok(OwnedFd::from_raw_fd(fd))
    }
}
