#!/bin/bash
# Builds target/mac/GridCraft.app through Xcode (macos/project.yml): the app with its App
# Intents metadata and its extensions. Needs XcodeGen (`brew install xcodegen`).
#
#   macos/build-xcode.sh               # release; builds the engine first unless SKIP_ENGINE=1
#   CONFIG=Debug macos/build-xcode.sh
#   ARCHS=arm64 macos/build-xcode.sh  # this Mac's architecture only (as the engine was built)
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CONFIG=${CONFIG:-Release}
OUT="$ROOT/target/mac"

if [ "${SKIP_ENGINE:-0}" != 1 ]; then
    PROFILE=$([ "$CONFIG" = Debug ] && echo dev || echo release) "$ROOT/macos/build-engine.sh"
fi

echo "== Xcode project"
(cd "$ROOT/macos" && xcodegen generate --quiet --spec project.yml)

echo "== GridCraft.app ($CONFIG)"
# A fresh app every time, so an old one is never copied.
rm -rf "$OUT/xcode/Build/Products/$CONFIG/GridCraft.app"
xcodebuild -project "$ROOT/macos/GridCraft.xcodeproj" -scheme GridCraftApp -configuration "$CONFIG" \
    -derivedDataPath "$OUT/xcode" -destination 'platform=macOS' ARCHS="${ARCHS:-arm64 x86_64}" build -quiet
mkdir -p "$OUT"
rm -rf "$OUT/GridCraft.app"
APP="$OUT/xcode/Build/Products/$CONFIG/GridCraft.app"
[ -x "$APP/Contents/MacOS/GridCraft" ] || { echo "error: xcodebuild made no GridCraft.app" >&2; exit 1; }
cp -R "$APP" "$OUT/GridCraft.app"
echo "== done: $OUT/GridCraft.app"
