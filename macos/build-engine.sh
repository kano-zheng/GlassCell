#!/bin/bash
# Builds the GridCraft engine for the Mac app: the `gridcraft-ffi` static library for Apple
# silicon and Intel in an XCFramework, and its Swift bindings, into macos/GridCraftEngine.
#
#   macos/build-engine.sh              # release, arm64 + x86_64
#   PROFILE=dev macos/build-engine.sh  # faster debug build
#   ARCHS=arm64 macos/build-engine.sh  # this Mac's architecture only
#
# Neither output is committed (see .gitignore); run this after changing the engine or the bridge.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
PKG="$ROOT/macos/GridCraftEngine"
PROFILE=${PROFILE:-release}
ARCHS=${ARCHS:-"arm64 x86_64"}
DIR=$([ "$PROFILE" = dev ] && echo debug || echo "$PROFILE")
TARGET_DIR=${CARGO_TARGET_DIR:-"$ROOT/target"}
WORK="$TARGET_DIR/gridcraft-ffi"

# Use rustup's toolchain when there is one: another cargo on PATH (Homebrew's) lacks the targets.
CARGO=cargo
if command -v rustup >/dev/null; then
    CARGO=$(rustup which cargo)
    export RUSTC
    RUSTC=$(rustup which rustc)
fi

libs=()
for arch in $ARCHS; do
    case $arch in
        arm64) triple=aarch64-apple-darwin ;;
        x86_64) triple=x86_64-apple-darwin ;;
        *) echo "unknown architecture $arch" >&2; exit 1 ;;
    esac
    command -v rustup >/dev/null && rustup target add "$triple" >/dev/null
    echo "== gridcraft-ffi for $triple ($PROFILE)"
    MACOSX_DEPLOYMENT_TARGET=26.0 "$CARGO" build --manifest-path "$ROOT/Cargo.toml" -p gridcraft-ffi --profile "$PROFILE" --target "$triple" --target-dir "$TARGET_DIR"
    libs+=("$TARGET_DIR/$triple/$DIR/libgridcraft_ffi.a")
done

rm -rf "$WORK"
mkdir -p "$WORK/headers" "$WORK/swift"
lipo -create "${libs[@]}" -output "$WORK/libgridcraft_ffi.a"

echo "== Swift bindings"
"$CARGO" run --manifest-path "$ROOT/Cargo.toml" -q -p gridcraft-ffi --features bindgen --bin uniffi-bindgen --target-dir "$TARGET_DIR" -- \
    generate --library "${libs[0]}" --language swift --out-dir "$WORK/swift"
cp "$WORK/swift/gridcraft_ffiFFI.h" "$WORK/headers/"
# An XCFramework's headers carry their module map as module.modulemap.
cp "$WORK/swift/gridcraft_ffiFFI.modulemap" "$WORK/headers/module.modulemap"

echo "== XCFramework"
rm -rf "$PKG/GridCraftFFI.xcframework"
xcodebuild -create-xcframework -library "$WORK/libgridcraft_ffi.a" -headers "$WORK/headers" -output "$PKG/GridCraftFFI.xcframework" >/dev/null
cp "$WORK/swift/gridcraft_ffi.swift" "$PKG/Sources/GridCraftEngine/Generated.swift"
echo "== done: $PKG"
