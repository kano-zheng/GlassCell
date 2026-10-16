#!/bin/bash
# Builds GridCraft.app (the SwiftUI app with its document types) into target/mac/.
#
#   macos/build-app.sh          # release; builds the engine first unless SKIP_ENGINE=1
#   CONFIG=debug macos/build-app.sh
#   open target/mac/GridCraft.app
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CONFIG=${CONFIG:-release}
OUT="$ROOT/target/mac"
APP="$OUT/GridCraft.app"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)

if [ "${SKIP_ENGINE:-0}" != 1 ]; then
    PROFILE=$([ "$CONFIG" = debug ] && echo dev || echo release) "$ROOT/macos/build-engine.sh"
fi

echo "== GridCraft ($CONFIG)"
swift build --package-path "$ROOT/macos/GridCraft" -c "$CONFIG" --product GridCraft
BIN=$(swift build --package-path "$ROOT/macos/GridCraft" -c "$CONFIG" --show-bin-path)

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN/GridCraft" "$APP/Contents/MacOS/GridCraft"
sed "s/__VERSION__/$VERSION/g" "$ROOT/macos/GridCraft/Resources/Info.plist" > "$APP/Contents/Info.plist"
# SwiftPM resource bundles, if any target gains resources.
find "$BIN" -maxdepth 1 -name '*.bundle' -exec cp -R {} "$APP/Contents/Resources/" \;
codesign --force --sign - --timestamp=none "$APP" >/dev/null
echo "== done: $APP"
