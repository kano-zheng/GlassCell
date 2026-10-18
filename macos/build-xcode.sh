#!/bin/bash
# Builds target/mac/GlassCell.app through Xcode (macos/project.yml): the app with its App
# Intents metadata and its extensions. Needs XcodeGen (`brew install xcodegen`).
#
#   macos/build-xcode.sh               # release; builds the engine first unless SKIP_ENGINE=1
#   CONFIG=Debug macos/build-xcode.sh
#   ARCHS=arm64 macos/build-xcode.sh  # this Mac's architecture only (as the engine was built)
#   MARKETING_VERSION=0.1.0 BUILD_NUMBER=42 macos/build-xcode.sh  # a release's version
#   INSTALL=1 macos/build-xcode.sh  # and copy the app into /Applications afterwards
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CONFIG=${CONFIG:-Release}
OUT="$ROOT/target/mac"

if [ "${SKIP_ENGINE:-0}" != 1 ]; then
    PROFILE=$([ "$CONFIG" = Debug ] && echo dev || echo release) "$ROOT/macos/build-engine.sh"
fi

echo "== Xcode project"
(cd "$ROOT/macos" && xcodegen generate --quiet --spec project.yml)

echo "== GlassCell.app ($CONFIG)"
# A fresh app every time, so an old one is never copied.
rm -rf "$OUT/xcode/Build/Products/$CONFIG/GlassCell.app"
VERSION_ARGS=()
[ -n "${MARKETING_VERSION:-}" ] && VERSION_ARGS+=("MARKETING_VERSION=$MARKETING_VERSION")
[ -n "${BUILD_NUMBER:-}" ] && VERSION_ARGS+=("CURRENT_PROJECT_VERSION=$BUILD_NUMBER")
xcodebuild -project "$ROOT/macos/GlassCell.xcodeproj" -scheme GlassCell -configuration "$CONFIG" \
    -derivedDataPath "$OUT/xcode" -destination 'platform=macOS' ARCHS="${ARCHS:-arm64 x86_64}" \
    ${VERSION_ARGS[@]+"${VERSION_ARGS[@]}"} build -quiet
mkdir -p "$OUT"
rm -rf "$OUT/GlassCell.app"
APP="$OUT/xcode/Build/Products/$CONFIG/GlassCell.app"
[ -x "$APP/Contents/MacOS/GlassCell" ] || { echo "error: xcodebuild made no GlassCell.app" >&2; exit 1; }
cp -R "$APP" "$OUT/GlassCell.app"
echo "== done: $OUT/GlassCell.app"
[ "${INSTALL:-0}" != 1 ] || "$ROOT/macos/install-app.sh"
