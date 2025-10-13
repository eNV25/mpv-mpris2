use std::os::fd::{BorrowedFd, OwnedFd};

pub(crate) fn mpv_ipc_fd() -> anyhow::Result<OwnedFd> {
    let mut args = pico_args::Arguments::from_env();
    let fd = args.value_from_str("--mpv-ipc-fd")?;
    let fd = unsafe { BorrowedFd::borrow_raw(fd) };
    Ok(fd.try_clone_to_owned()?)
}
