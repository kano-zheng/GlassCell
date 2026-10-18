#!/bin/sh
# Release build of GlassCell for the web into dist/glasscell-web: size-optimised wasm
# (opt-level s, fat LTO, one codegen unit), about 40% smaller than the workspace's release
# profile. Needs `trunk` and the wasm32-unknown-unknown target. Extra arguments go to trunk.
cd "$(dirname "$0")" || exit 1
export CARGO_PROFILE_RELEASE_OPT_LEVEL=s CARGO_PROFILE_RELEASE_LTO=fat CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
# rustup's rust-lld looks for libLLVM next to the compiler's libraries; this finds it on macOS
# (the variable is set here because /bin/sh drops it from the environment it is started in).
export DYLD_FALLBACK_LIBRARY_PATH="$(rustc --print sysroot)/lib${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}"
exec trunk build --release "$@"
