# mpris.so

This mpv plugin implements the MPRIS v2 DBus interface. The MPRIS API is used by
Linux desktop environments and tools like `playerctl` to control media player
and provide metadata.

This implementation has been tested with KDE Plasma and `playerctl`.

There is no relation [@hoyon's mpv-mpris](https://github.com/hoyon/mpv-mpris).

## Installation

The plugin must be installed to the `scripts/` subdirectory of the mpv configuration
directory. See the mpv manual to find the configuration directory. The filename `mpris.so` is recommended.
Pre-build binary is available in GitHub.

Any other implementations of the MPRIS API (like [hoyon/mpv-mpris](https://github.com/hoyon/mpv-mpris)) must be uninstalled.

### Building from source

```
$ git clone https://github.com/eNV25/mpv-mpris2
$ cargo build --release
$ install -v target/release/libmpv_mpris.so ~/.config/mpv/scripts/mpris.so
```
