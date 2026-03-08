# Whispr — Local Voice-to-Text for macOS

## Overview

A Tauri 2.0 desktop app that provides system-wide voice dictation using local AI models. Press a global hotkey to start recording, speak, press Escape to stop, and cleaned-up text is pasted into the active application.

## Core Interaction

1. Press global hotkey (Cmd+Shift+Space) → recording starts
2. Live transcription appears in a floating overlay as you speak
3. Press Escape → recording stops, final transcript is reformatted via Qwen
4. Cleaned text is written to clipboard and pasted into the previously active app

## Architecture

### Components

- **Menubar/tray icon** — shows recording state (idle / recording / processing)
- **Floating overlay** — small transparent window showing live transcription
- **Audio capture** — records mic via `cpal` into WAV buffer
- **Whisper bridge** — `whisper-rs` bindings to whisper.cpp for local STT
- **Context collector** — detects focused app/project, scans file tree (respects .gitignore), builds context string
- **Qwen bridge** — HTTP call to local Ollama for transcript cleanup/reformatting
- **Paste module** — writes to clipboard, simulates Cmd+V via CGEvent

### Data Flow

```
Hotkey → start mic capture + show overlay
  → stream audio chunks to Whisper → live transcript in overlay
Escape → stop capture
  → final Whisper transcribe → raw text
  → detect project context (file tree of focused project)
  → Qwen reformat (raw text + context) → clean text
  → clipboard + simulate paste into previously active app
  → hide overlay
```

### Tech Stack

- **Tauri 2.0** — app shell, global shortcut plugin, tray icon
- **whisper-rs** — Rust bindings to whisper.cpp
- **cpal** — cross-platform audio capture in Rust
- **reqwest** — HTTP client for Ollama API
- **Minimal frontend** — floating overlay UI (HTML/CSS/JS)

### Key Behaviors

- Overlay floats above all windows, transparent background, just shows text
- When recording starts, remember which app was focused (to paste back into it)
- File tree context is gathered from the focused app's working directory if detectable
- Qwen prompt includes raw transcript + file listing for smart name resolution

## Out of Scope

- LSP integration (future)
- Model selection UI
- Settings UI (hardcoded config)
- Audio playback / waveform
