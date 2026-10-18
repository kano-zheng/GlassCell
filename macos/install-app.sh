#!/bin/bash
# Copies target/mac/GlassCell.app into /Applications (or $DEST), replacing the app there.
# The build scripts call it when INSTALL=1; it can also be run by hand after a build.
#
#   INSTALL=1 macos/build-xcode.sh
#   macos/install-app.sh
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
SRC="$ROOT/target/mac/GlassCell.app"
DEST=${DEST:-/Applications}

[ -x "$SRC/Contents/MacOS/GlassCell" ] || { echo "error: $SRC is not built" >&2; exit 1; }

# Copy beside the old app first, so a failed copy never leaves the Applications folder without one.
rm -rf "$DEST/.GlassCell.app.new"
ditto "$SRC" "$DEST/.GlassCell.app.new"
rm -rf "$DEST/GlassCell.app"
/bin/mv -f "$DEST/.GlassCell.app.new" "$DEST/GlassCell.app"
echo "== installed: $DEST/GlassCell.app"
