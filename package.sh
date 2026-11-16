#!/usr/bin/env bash
# Builds the .deb of Soplos Theme Manager and moves it to the shared
# Descargas folder and to the MEGA folder of the 3.0 apps, then removes the
# bundle directory of the build so old .deb files do not pile up there.
#
# Usage: ./package.sh

set -e

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST_DIR="$HOME/Descargas/SoplosApps"
MEGA_DIR="$HOME/MEGA/Soplos Apps/Soplos Apps 3.0"
BUNDLE_DIR="$PROJECT_DIR/src-tauri/target/release/bundle"

cd "$PROJECT_DIR"
npm run tauri build

DEB="$(find "$BUNDLE_DIR/deb" -maxdepth 1 -name '*.deb' -print -quit)"
[[ -n "$DEB" ]] || { echo "ERROR: no .deb found in $BUNDLE_DIR/deb" >&2; exit 1; }

mkdir -p "$DEST_DIR" "$MEGA_DIR"
cp "$DEB" "$DEST_DIR/"
echo "Copied: $DEST_DIR/$(basename "$DEB")"
cp "$DEB" "$MEGA_DIR/"
echo "Copied: $MEGA_DIR/$(basename "$DEB")"

rm -rf "$BUNDLE_DIR"
echo "Cleaned up: $BUNDLE_DIR"
