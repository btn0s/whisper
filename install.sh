#!/bin/bash
set -euo pipefail

REPO="btn0s/whisper"
APP_NAME="whisper"
MODEL_DIR="$HOME/.whisper"
MODEL_PATH="$MODEL_DIR/ggml-base.en.bin"
MODEL_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin"

echo "Installing $APP_NAME..."

# 1. Get latest release DMG URL
DMG_URL=$(curl -sL "https://api.github.com/repos/$REPO/releases/latest" \
  | grep "browser_download_url.*\.dmg" \
  | head -1 \
  | cut -d '"' -f 4)

if [ -z "$DMG_URL" ]; then
  echo "Error: Could not find DMG in latest release"
  exit 1
fi

echo "Downloading $DMG_URL..."
TMP_DMG=$(mktemp /tmp/whisper-XXXXXX.dmg)
curl -sL "$DMG_URL" -o "$TMP_DMG"

# 2. Mount, copy to /Applications, unmount
echo "Installing to /Applications..."
MOUNT_POINT=$(hdiutil attach "$TMP_DMG" -nobrowse -quiet | tail -1 | awk '{print $NF}')
# Find the .app in the mounted volume
APP_PATH=$(find "$MOUNT_POINT" -maxdepth 1 -name "*.app" | head -1)
if [ -z "$APP_PATH" ]; then
  hdiutil detach "$MOUNT_POINT" -quiet
  rm -f "$TMP_DMG"
  echo "Error: No .app found in DMG"
  exit 1
fi
rm -rf "/Applications/$APP_NAME.app"
cp -R "$APP_PATH" "/Applications/$APP_NAME.app"
hdiutil detach "$MOUNT_POINT" -quiet
rm -f "$TMP_DMG"

# 3. Download Whisper model if not present
if [ -f "$MODEL_PATH" ]; then
  echo "Whisper model already exists at $MODEL_PATH"
else
  echo "Downloading Whisper model (~142MB)..."
  mkdir -p "$MODEL_DIR"
  curl -L "$MODEL_URL" -o "$MODEL_PATH"
fi

echo ""
echo "Done! $APP_NAME installed to /Applications."
echo ""
echo "To start: open /Applications/$APP_NAME.app"
echo ""
echo "You'll need to grant these permissions on first launch:"
echo "  - Microphone (System Settings > Privacy & Security > Microphone)"
echo "  - Accessibility (System Settings > Privacy & Security > Accessibility)"
