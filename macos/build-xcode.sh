#!/bin/bash
# Builds target/mac/GridCraft.app through Xcode (macos/project.yml): the app with its App
# Intents metadata and its extensions. Needs XcodeGen (`brew install xcodegen`).
#
#   macos/build-xcode.sh               # release; builds the engine first unless SKIP_ENGINE=1
#   CONFIG=Debug macos/build-xcode.sh
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
xcodebuild -project "$ROOT/macos/GridCraft.xcodeproj" -scheme GridCraft -configuration "$CONFIG" \
    -derivedDataPath "$OUT/xcode" -destination 'platform=macOS' build -quiet
mkdir -p "$OUT"
rm -rf "$OUT/GridCraft.app"
cp -R "$OUT/xcode/Build/Products/$CONFIG/GridCraft.app" "$OUT/GridCraft.app"
echo "== done: $OUT/GridCraft.app"
