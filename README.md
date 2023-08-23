# mpris.so

This mpv plugin implements the MPRIS v2 DBus interface. The MPRIS API is used by
Linux desktop environments and tools like `playerctl` to control media player
and provide metadata.

This implementation has been tested with KDE Plasma and `playerctl`.

## Installation

The plugin must be installed to the `scripts/` subdirectory of the mpv configuration
directory. See the mpv manual to find the configuration directory. The filename `mpris.so` is recommended.
Pre-build binary is available in GitHub.

Any other implementations of the MPRIS API (like [hoyon/mpv-mpris](https://github.com/hoyon/mpv-mpris)) must be uninstalled.

