# whisper

Local, on-device voice-to-text for macOS. Press a hotkey, speak, get text pasted into any app.

Uses [whisper.cpp](https://github.com/ggerganov/whisper.cpp) for fully local speech-to-text — no network requests, no cloud APIs.

## Install

```bash
curl -sL https://raw.githubusercontent.com/btn0s/whisper/main/install.sh | bash
```

This downloads the app to `/Applications`, and fetches the Whisper model (~142MB) to `~/.whisper/`.

On first launch, grant microphone and accessibility permissions when prompted.

## Usage

- **Option+Space** — Toggle recording (press to start, press again to stop and paste)
- **Escape** — Cancel recording (only captured while recording)
- Lives in the menubar tray

## Dev

### Prerequisites

- Rust / Cargo
- CMake (for whisper.cpp compilation)

### Run

```bash
cargo tauri dev
```

### Build

```bash
CMAKE_OSX_DEPLOYMENT_TARGET=10.15 cargo tauri build
```

## Config

- `WHISPER_MODEL` — Path to Whisper `.bin` model (default: `~/.whisper/ggml-base.en.bin`)
