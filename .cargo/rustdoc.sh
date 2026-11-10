#!/bin/sh
# rustdoc, its doc tests linked with an rpath to the toolchain's libstd.
# With `dev`, Bevy links dynamically (`dynamic_linking`), so a doc test needs
# libstd's shared object, and rustdoc runs it without the toolchain's
# library directory on the loader's path.
exec rustdoc -Clink-arg=-Wl,-rpath,"$(rustc --print target-libdir)" "$@"
