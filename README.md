# whisper

Local, on-device voice-to-text for macOS. Press a hotkey, speak, get text pasted into any app.

Uses [whisper.cpp](https://github.com/ggerganov/whisper.cpp) for fully local speech-to-text — no network requests, no cloud APIs.

## Install

1. Download the latest `.dmg` from [Releases](https://github.com/btn0s/whisper/releases)
2. Open the DMG and drag whisper to Applications
3. Download a Whisper model:
   ```bash
   mkdir -p ~/.whisper && curl -L -o ~/.whisper/ggml-base.en.bin \
     https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin
   ```
4. Grant microphone and accessibility permissions when prompted

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
