# mpris.run

This mpv plugin implements the MPRIS v2 DBus interface. The MPRIS API is used by
Linux desktop environments and tools like `playerctl` to control media player
and provide metadata.

This implementation has been tested with GNOME and `playerctl`.

There is no relation [@hoyon's mpv-mpris](https://github.com/hoyon/mpv-mpris).

## Install

The plugin must be installed to the `scripts/` subdirectory of the mpv configuration
directory. See the mpv manual to find the configuration directory. The filename `mpris.run`
is recommended, but the `.run` extension is required.

### Build from source

```
$ git clone https://github.com/eNV25/mpv-mpris2
$ cargo build --release
$ install -v target/release/mpv-mpris2 ~/.config/mpv/scripts/mpris.run
```
