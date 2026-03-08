# Whispr

Local voice-to-text for macOS. Press a hotkey, speak, get clean text pasted into any app.

Uses Whisper (local) for speech-to-text and Qwen via Ollama (local) for transcript cleanup.

## Prerequisites

- Rust / Cargo
- CMake (for whisper.cpp compilation)
- Ollama (https://ollama.com)

## Setup

```bash
chmod +x setup.sh && ./setup.sh
```

This downloads the Whisper model and pulls Qwen via Ollama.

## Run

```bash
cargo tauri dev
```

## Usage

- **Cmd+Shift+Space** — Start recording
- **Escape** — Stop recording, process, and paste result
- Lives in the menubar tray

## Config (env vars)

- `WHISPR_MODEL` — Path to Whisper .bin model (default: `~/.whispr/ggml-base.en.bin`)
- `WHISPR_LLM_MODEL` — Ollama model name (default: `qwen3.5:8b`)
