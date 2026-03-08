# Whispr Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a local macOS voice-to-text app using Tauri 2.0, whisper.cpp, and Qwen via Ollama.

**Architecture:** Tauri 2.0 desktop app with a Rust backend handling audio capture (cpal), transcription (whisper-rs), and LLM reformatting (Ollama HTTP). A transparent floating overlay shows live transcription. Global hotkey toggles recording, Escape stops it.

**Tech Stack:** Tauri 2.0, whisper-rs 0.15, cpal 0.15, reqwest, core-graphics, vanilla HTML/CSS/JS frontend

---

### Task 1: Scaffold Tauri 2.0 Project

**Files:**
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/main.rs`
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/capabilities/default.json`
- Create: `src-tauri/build.rs`
- Create: `src/index.html`
- Create: `src/overlay.html`
- Create: `src/styles.css`
- Create: `.gitignore`

**Step 1: Create project structure**

`.gitignore`:
```
/src-tauri/target/
*.ggml
*.bin
```

`src-tauri/build.rs`:
```rust
fn main() {
    tauri_build::build()
}
```

`src-tauri/Cargo.toml`:
```toml
[package]
name = "whispr"
version = "0.1.0"
edition = "2021"

[dependencies]
tauri = { version = "2", features = ["tray-icon", "image-png"] }
tauri-plugin-global-shortcut = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.12", features = ["json"] }
whisper-rs = "0.15"
cpal = "0.15"
core-graphics = "0.24"
tokio = { version = "1", features = ["sync"] }
arboard = "3"

[build-dependencies]
tauri-build = { version = "2", features = [] }
```

`src-tauri/tauri.conf.json`:
```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Whispr",
  "version": "0.1.0",
  "identifier": "com.whispr.app",
  "build": {
    "beforeBuildCommand": "",
    "beforeDevCommand": "",
    "frontendDist": "../src"
  },
  "app": {
    "withGlobalTauri": true,
    "windows": [
      {
        "label": "overlay",
        "title": "Whispr",
        "url": "overlay.html",
        "width": 500,
        "height": 120,
        "transparent": true,
        "decorations": false,
        "alwaysOnTop": true,
        "resizable": false,
        "visible": false,
        "skipTaskbar": true
      }
    ],
    "security": {
      "csp": "default-src 'self'; style-src 'self' 'unsafe-inline'; connect-src ipc: http://ipc.localhost"
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  },
  "plugins": {
    "global-shortcut": {}
  }
}
```

`src-tauri/capabilities/default.json`:
```json
{
  "identifier": "default",
  "description": "Default capabilities",
  "windows": ["*"],
  "permissions": [
    "core:default",
    "core:window:default",
    "core:window:allow-show",
    "core:window:allow-hide",
    "core:window:allow-set-focus",
    "global-shortcut:allow-register",
    "global-shortcut:allow-unregister"
  ]
}
```

`src/overlay.html`:
```html
<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8" />
  <link rel="stylesheet" href="styles.css" />
</head>
<body>
  <div id="overlay">
    <div id="status">
      <span id="indicator"></span>
      <span id="status-text">Listening...</span>
    </div>
    <div id="transcript"></div>
  </div>
  <script>
    const { listen } = window.__TAURI__.event;

    const transcript = document.getElementById('transcript');
    const indicator = document.getElementById('indicator');
    const statusText = document.getElementById('status-text');

    listen('transcription-update', (event) => {
      transcript.textContent = event.payload;
    });

    listen('recording-state', (event) => {
      const state = event.payload;
      indicator.className = state;
      if (state === 'recording') statusText.textContent = 'Listening...';
      else if (state === 'processing') statusText.textContent = 'Processing...';
      else statusText.textContent = '';
    });
  </script>
</body>
</html>
```

`src/styles.css`:
```css
* { margin: 0; padding: 0; box-sizing: border-box; }

html, body {
  background: transparent;
  font-family: -apple-system, BlinkMacSystemFont, sans-serif;
  overflow: hidden;
}

#overlay {
  background: rgba(28, 28, 30, 0.92);
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
  border-radius: 12px;
  padding: 16px 20px;
  margin: 8px;
  border: 1px solid rgba(255, 255, 255, 0.08);
}

#status {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}

#indicator {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #555;
}

#indicator.recording {
  background: #ff3b30;
  animation: pulse 1.5s ease-in-out infinite;
}

#indicator.processing {
  background: #ff9f0a;
  animation: pulse 0.8s ease-in-out infinite;
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.4; }
}

#status-text {
  font-size: 12px;
  color: rgba(255, 255, 255, 0.5);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  font-weight: 500;
}

#transcript {
  font-size: 15px;
  color: rgba(255, 255, 255, 0.9);
  line-height: 1.5;
  min-height: 1.5em;
}
```

`src/index.html`:
```html
<!DOCTYPE html>
<html><head><meta charset="utf-8" /></head>
<body></body>
</html>
```

`src-tauri/src/main.rs` (minimal skeleton):
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .run(tauri::generate_context!())
        .expect("error while running whispr");
}
```

**Step 2: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: compiles (warnings OK)

**Step 3: Commit**

```bash
git add -A
git commit -m "feat: scaffold Tauri 2.0 project structure"
```

---

### Task 2: Audio Capture Module

**Files:**
- Create: `src-tauri/src/audio.rs`
- Modify: `src-tauri/src/main.rs`

**Step 1: Write audio capture module**

`src-tauri/src/audio.rs`:
```rust
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

pub struct AudioCapture {
    stream: Option<cpal::Stream>,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    channels: u16,
}

impl AudioCapture {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("No input device available")?;

        let config = device.default_input_config()?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels();

        Ok(Self {
            stream: None,
            samples: Arc::new(Mutex::new(Vec::new())),
            sample_rate,
            channels,
        })
    }

    pub fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("No input device available")?;

        let config = device.default_input_config()?;

        // Clear previous samples
        self.samples.lock().unwrap().clear();

        let samples = self.samples.clone();
        let err_fn = |err| eprintln!("Audio stream error: {}", err);

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    samples.lock().unwrap().extend_from_slice(data);
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::I16 => {
                device.build_input_stream(
                    &config.into(),
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let converted: Vec<f32> =
                            data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                        samples.lock().unwrap().extend_from_slice(&converted);
                    },
                    err_fn,
                    None,
                )?
            }
            format => return Err(format!("Unsupported sample format: {:?}", format).into()),
        };

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop(&mut self) -> Vec<f32> {
        self.stream = None; // dropping stops the stream
        let raw = self.samples.lock().unwrap().clone();
        convert_to_whisper_format(&raw, self.sample_rate, self.channels)
    }

    /// Get a snapshot of current audio for live transcription without stopping.
    pub fn snapshot(&self) -> Vec<f32> {
        let raw = self.samples.lock().unwrap().clone();
        convert_to_whisper_format(&raw, self.sample_rate, self.channels)
    }
}

/// Convert captured audio to 16kHz mono f32 for Whisper.
fn convert_to_whisper_format(input: &[f32], sample_rate: u32, channels: u16) -> Vec<f32> {
    const TARGET_RATE: u32 = 16_000;

    // Mix to mono
    let mono: Vec<f32> = input
        .chunks(channels as usize)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();

    if sample_rate == TARGET_RATE {
        return mono;
    }

    // Resample via linear interpolation
    let ratio = sample_rate as f64 / TARGET_RATE as f64;
    let output_len = (mono.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_idx = i as f64 * ratio;
        let idx = src_idx as usize;
        let frac = src_idx - idx as f64;

        let sample = if idx + 1 < mono.len() {
            mono[idx] as f64 * (1.0 - frac) + mono[idx + 1] as f64 * frac
        } else if idx < mono.len() {
            mono[idx] as f64
        } else {
            0.0
        };
        output.push(sample as f32);
    }

    output
}
```

**Step 2: Register module in main.rs**

Add `mod audio;` to `main.rs`.

**Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check`

**Step 4: Commit**

```bash
git add src-tauri/src/audio.rs src-tauri/src/main.rs
git commit -m "feat: add audio capture module with resampling"
```

---

### Task 3: Whisper Transcription Module

**Files:**
- Create: `src-tauri/src/transcribe.rs`
- Modify: `src-tauri/src/main.rs`

**Step 1: Write transcription module**

`src-tauri/src/transcribe.rs`:
```rust
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};
use std::path::Path;

pub struct Transcriber {
    ctx: WhisperContext,
}

impl Transcriber {
    pub fn new(model_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        if !Path::new(model_path).exists() {
            return Err(format!("Whisper model not found at: {}", model_path).into());
        }
        let ctx = WhisperContext::new_with_params(
            model_path,
            WhisperContextParameters::default(),
        )?;
        Ok(Self { ctx })
    }

    /// Transcribe audio samples (16kHz mono f32).
    pub fn transcribe(&self, audio: &[f32]) -> Result<String, Box<dyn std::error::Error>> {
        if audio.is_empty() {
            return Ok(String::new());
        }

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_no_context(true);
        params.set_single_segment(false);

        let mut state = self.ctx.create_state()?;
        state.full(params, audio)?;

        let num_segments = state.full_n_segments()?;
        let mut text = String::new();
        for i in 0..num_segments {
            let segment = state.full_get_segment_text(i)?;
            text.push_str(segment.trim());
            text.push(' ');
        }

        Ok(text.trim().to_string())
    }
}
```

**Step 2: Register module in main.rs**

Add `mod transcribe;` to `main.rs`.

**Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check`

**Step 4: Commit**

```bash
git add src-tauri/src/transcribe.rs src-tauri/src/main.rs
git commit -m "feat: add Whisper transcription module"
```

---

### Task 4: Ollama / Qwen Integration

**Files:**
- Create: `src-tauri/src/llm.rs`
- Modify: `src-tauri/src/main.rs`

**Step 1: Write LLM module**

`src-tauri/src/llm.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

pub struct LlmClient {
    client: reqwest::Client,
    model: String,
    base_url: String,
}

impl LlmClient {
    pub fn new(model: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            model: model.to_string(),
            base_url: "http://localhost:11434".to_string(),
        }
    }

    /// Reformat raw transcript using project context.
    pub async fn reformat(
        &self,
        raw_transcript: &str,
        file_context: Option<&str>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let system = format!(
            "You are a dictation assistant. Clean up the following raw speech transcript into \
             well-formed text. Fix grammar, punctuation, and formatting. Preserve the speaker's \
             intent exactly — do not add, remove, or rephrase content.\n\
             \n\
             If the transcript references code or file names, use the project context below to \
             resolve them to their correct names.\n\
             \n\
             Output ONLY the cleaned text, nothing else.\n\
             \n\
             {}",
            file_context
                .map(|ctx| format!("Project files:\n{}", ctx))
                .unwrap_or_default()
        );

        let request = OllamaRequest {
            model: self.model.clone(),
            prompt: raw_transcript.to_string(),
            stream: false,
            system: Some(system),
        };

        let resp = self
            .client
            .post(format!("{}/api/generate", self.base_url))
            .json(&request)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(format!("Ollama returned status: {}", resp.status()).into());
        }

        let body: OllamaResponse = resp.json().await?;
        Ok(body.response.trim().to_string())
    }
}
```

**Step 2: Register module in main.rs**

Add `mod llm;` to `main.rs`.

**Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check`

**Step 4: Commit**

```bash
git add src-tauri/src/llm.rs src-tauri/src/main.rs
git commit -m "feat: add Ollama/Qwen LLM client for transcript reformatting"
```

---

### Task 5: Context Collector (Project File Tree)

**Files:**
- Create: `src-tauri/src/context.rs`
- Modify: `src-tauri/src/main.rs`

**Step 1: Write context collector**

`src-tauri/src/context.rs`:
```rust
use std::path::{Path, PathBuf};
use std::process::Command;

/// Get the project file tree as a string for LLM context.
/// Scans the given directory, respecting .gitignore via `git ls-files`.
pub fn collect_file_tree(project_root: &Path) -> Option<String> {
    if !project_root.is_dir() {
        return None;
    }

    // Try git ls-files first (respects .gitignore)
    let output = Command::new("git")
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .current_dir(project_root)
        .output()
        .ok()?;

    if output.status.success() {
        let files = String::from_utf8_lossy(&output.stdout);
        let truncated: String = files
            .lines()
            .take(200) // cap at 200 files to keep context manageable
            .collect::<Vec<_>>()
            .join("\n");
        return Some(truncated);
    }

    // Fallback: simple directory scan (top 2 levels)
    let mut files = Vec::new();
    collect_recursive(project_root, project_root, 0, 2, &mut files);
    files.truncate(200);
    Some(files.join("\n"))
}

fn collect_recursive(
    base: &Path,
    dir: &Path,
    depth: usize,
    max_depth: usize,
    out: &mut Vec<String>,
) {
    if depth > max_depth {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();

        // Skip hidden dirs and common noise
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }

        if let Ok(rel) = path.strip_prefix(base) {
            out.push(rel.to_string_lossy().to_string());
        }

        if path.is_dir() {
            collect_recursive(base, &path, depth + 1, max_depth, out);
        }
    }
}

/// Try to detect the project root from the currently focused app.
/// On macOS, attempts to get the frontmost app's recent document path.
/// Falls back to the user's home directory.
pub fn detect_project_root() -> Option<PathBuf> {
    // Try to get frontmost app's working directory via lsof
    let output = Command::new("sh")
        .args([
            "-c",
            "lsof -p $(osascript -e 'tell application \"System Events\" to unix id of first process whose frontmost is true') 2>/dev/null | grep cwd | awk '{print $NF}'"
        ])
        .output()
        .ok()?;

    if output.status.success() {
        let cwd = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !cwd.is_empty() && Path::new(&cwd).is_dir() {
            return Some(PathBuf::from(cwd));
        }
    }

    None
}
```

**Step 2: Register module in main.rs**

Add `mod context;` to `main.rs`.

**Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check`

**Step 4: Commit**

```bash
git add src-tauri/src/context.rs src-tauri/src/main.rs
git commit -m "feat: add project context collector for file tree awareness"
```

---

### Task 6: Paste Module

**Files:**
- Create: `src-tauri/src/paste.rs`
- Modify: `src-tauri/src/main.rs`

**Step 1: Write paste module**

`src-tauri/src/paste.rs`:
```rust
use arboard::Clipboard;
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

/// Virtual key code for 'V' on macOS
const V_KEYCODE: CGKeyCode = 9;

/// Set clipboard text and simulate Cmd+V to paste into the active app.
pub fn paste_text(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Set clipboard
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(text)?;

    // Brief delay for clipboard to settle
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Simulate Cmd+V
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "Failed to create CGEventSource")?;

    let key_down = CGEvent::new_keyboard_event(source.clone(), V_KEYCODE, true)
        .map_err(|_| "Failed to create key down event")?;
    key_down.set_flags(CGEventFlags::CGEventFlagCommand);

    let key_up = CGEvent::new_keyboard_event(source, V_KEYCODE, false)
        .map_err(|_| "Failed to create key up event")?;
    key_up.set_flags(CGEventFlags::CGEventFlagCommand);

    key_down.post(CGEventTapLocation::HID);
    key_up.post(CGEventTapLocation::HID);

    Ok(())
}
```

**Step 2: Register module in main.rs**

Add `mod paste;` to `main.rs`.

**Step 3: Verify it compiles**

Run: `cd src-tauri && cargo check`

**Step 4: Commit**

```bash
git add src-tauri/src/paste.rs src-tauri/src/main.rs
git commit -m "feat: add clipboard paste module using CGEvent"
```

---

### Task 7: Wire Everything Together in main.rs

**Files:**
- Modify: `src-tauri/src/main.rs`

**Step 1: Write the full main.rs with app state and recording logic**

`src-tauri/src/main.rs`:
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod context;
mod llm;
mod paste;
mod transcribe;

use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

struct AppState {
    audio: Mutex<audio::AudioCapture>,
    transcriber: Arc<transcribe::Transcriber>,
    llm: llm::LlmClient,
    is_recording: Mutex<bool>,
}

fn main() {
    let model_path = std::env::var("WHISPR_MODEL")
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            format!("{}/.whispr/ggml-base.en.bin", home)
        });

    let qwen_model = std::env::var("WHISPR_LLM_MODEL")
        .unwrap_or_else(|_| "qwen2.5:7b".to_string());

    let transcriber = Arc::new(
        transcribe::Transcriber::new(&model_path)
            .expect("Failed to load Whisper model. Set WHISPR_MODEL env var or place model at ~/.whispr/ggml-base.en.bin")
    );

    let audio_capture = audio::AudioCapture::new()
        .expect("Failed to initialize audio capture");

    let state = AppState {
        audio: Mutex::new(audio_capture),
        transcriber,
        llm: llm::LlmClient::new(&qwen_model),
        is_recording: Mutex::new(false),
    };

    tauri::Builder::default()
        .manage(state)
        .setup(|app| {
            // --- Tray Icon ---
            let quit = MenuItem::with_id(app, "quit", "Quit Whispr", true, None::<&str>)?;
            let menu = MenuBuilder::new(app).items(&[&quit]).build()?;

            let _tray = TrayIconBuilder::with_id("whispr-tray")
                .tooltip("Whispr — Voice to Text")
                .menu(&menu)
                .on_menu_event(|app, event| {
                    if event.id().as_ref() == "quit" {
                        app.exit(0);
                    }
                })
                .build(app)?;

            // --- Global Shortcut: Cmd+Shift+Space to toggle ---
            let toggle_shortcut =
                Shortcut::new(Some(Modifiers::META | Modifiers::SHIFT), Code::Space);
            let escape_shortcut = Shortcut::new(None, Code::Escape);

            let app_handle = app.handle().clone();
            app_handle.plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(move |app, shortcut, event| {
                        if event.state() != ShortcutState::Pressed {
                            return;
                        }

                        if shortcut == &toggle_shortcut {
                            toggle_recording(app);
                        } else if shortcut == &escape_shortcut {
                            stop_and_process(app);
                        }
                    })
                    .build(),
            )?;

            app.global_shortcut().register(toggle_shortcut)?;
            app.global_shortcut().register(escape_shortcut)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running whispr");
}

fn toggle_recording(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let mut is_recording = state.is_recording.lock().unwrap();

    if *is_recording {
        drop(is_recording);
        stop_and_process(app);
    } else {
        // Start recording
        let mut audio = state.audio.lock().unwrap();
        if let Err(e) = audio.start() {
            eprintln!("Failed to start recording: {}", e);
            return;
        }
        *is_recording = true;

        // Show overlay
        if let Some(window) = app.get_webview_window("overlay") {
            let _ = window.show();
            let _ = window.set_focus();
            let _ = app.emit("recording-state", "recording");
        }

        // Start live transcription loop
        let app_handle = app.clone();
        let transcriber = state.transcriber.clone();
        std::thread::spawn(move || {
            live_transcription_loop(&app_handle, &transcriber);
        });
    }
}

fn live_transcription_loop(app: &tauri::AppHandle, transcriber: &transcribe::Transcriber) {
    loop {
        std::thread::sleep(Duration::from_secs(2));

        let state = app.state::<AppState>();
        let is_recording = state.is_recording.lock().unwrap();
        if !*is_recording {
            break;
        }
        drop(is_recording);

        let audio = state.audio.lock().unwrap();
        let snapshot = audio.snapshot();
        drop(audio);

        if snapshot.is_empty() {
            continue;
        }

        match transcriber.transcribe(&snapshot) {
            Ok(text) if !text.is_empty() => {
                let _ = app.emit("transcription-update", &text);
            }
            Err(e) => eprintln!("Live transcription error: {}", e),
            _ => {}
        }
    }
}

fn stop_and_process(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let mut is_recording = state.is_recording.lock().unwrap();
    if !*is_recording {
        return;
    }
    *is_recording = false;
    drop(is_recording);

    let _ = app.emit("recording-state", "processing");

    // Stop audio and get samples
    let mut audio = state.audio.lock().unwrap();
    let samples = audio.stop();
    drop(audio);

    // Final transcription + LLM reformat (async)
    let app_handle = app.clone();
    let transcriber = state.transcriber.clone();
    tauri::async_runtime::spawn(async move {
        let state = app_handle.state::<AppState>();

        // Transcribe
        let raw_text = match transcriber.transcribe(&samples) {
            Ok(text) => text,
            Err(e) => {
                eprintln!("Transcription error: {}", e);
                hide_overlay(&app_handle);
                return;
            }
        };

        if raw_text.is_empty() {
            hide_overlay(&app_handle);
            return;
        }

        let _ = app_handle.emit("transcription-update", &raw_text);

        // Collect project context
        let file_context = context::detect_project_root()
            .and_then(|root| context::collect_file_tree(&root));

        // Reformat with LLM
        let final_text = match state
            .llm
            .reformat(&raw_text, file_context.as_deref())
            .await
        {
            Ok(text) => text,
            Err(e) => {
                eprintln!("LLM reformat error: {}, using raw transcript", e);
                raw_text
            }
        };

        let _ = app_handle.emit("transcription-update", &final_text);

        // Brief pause so user can see final text
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Paste
        if let Err(e) = paste::paste_text(&final_text) {
            eprintln!("Paste error: {}", e);
        }

        hide_overlay(&app_handle);
    });
}

fn hide_overlay(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.hide();
    }
    let _ = app.emit("recording-state", "idle");
    let _ = app.emit("transcription-update", "");
}
```

**Step 2: Verify it compiles**

Run: `cd src-tauri && cargo check`

**Step 3: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat: wire up full recording pipeline — hotkey, capture, transcribe, reformat, paste"
```

---

### Task 8: Model Setup Script & README

**Files:**
- Create: `setup.sh`
- Create: `README.md`

**Step 1: Create setup script**

`setup.sh`:
```bash
#!/bin/bash
set -e

WHISPR_DIR="$HOME/.whispr"
MODEL_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin"
MODEL_PATH="$WHISPR_DIR/ggml-base.en.bin"

echo "=== Whispr Setup ==="

# Download whisper model
if [ ! -f "$MODEL_PATH" ]; then
    echo "Downloading Whisper base.en model..."
    mkdir -p "$WHISPR_DIR"
    curl -L "$MODEL_URL" -o "$MODEL_PATH"
    echo "Model saved to $MODEL_PATH"
else
    echo "Whisper model already exists at $MODEL_PATH"
fi

# Check Ollama
if ! command -v ollama &> /dev/null; then
    echo ""
    echo "WARNING: Ollama is not installed."
    echo "Install it from https://ollama.com and then run:"
    echo "  ollama pull qwen2.5:7b"
else
    echo ""
    echo "Pulling Qwen model via Ollama..."
    ollama pull qwen2.5:7b
fi

echo ""
echo "Setup complete! Run 'cargo tauri dev' from the project root."
```

**Step 2: Create README**

`README.md`:
```markdown
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

- **Cmd+Shift+Space** — Toggle recording on
- **Escape** — Stop recording, process, and paste result
- Lives in the menubar tray

## Config (env vars)

- `WHISPR_MODEL` — Path to Whisper .bin model (default: `~/.whispr/ggml-base.en.bin`)
- `WHISPR_LLM_MODEL` — Ollama model name (default: `qwen2.5:7b`)
```

**Step 3: Commit**

```bash
chmod +x setup.sh
git add setup.sh README.md
git commit -m "feat: add setup script and README"
```

---

### Task 9: Build & Smoke Test

**Step 1: Run setup**

```bash
./setup.sh
```

**Step 2: Build and run**

```bash
cargo tauri dev
```

**Step 3: Manual smoke test**

1. Verify tray icon appears in menubar
2. Press Cmd+Shift+Space — overlay should appear with "Listening..."
3. Speak a sentence
4. Press Escape — should see "Processing...", then text pastes
5. Open a text editor, repeat — verify text appears in editor

**Step 4: Fix any issues found during smoke test**

**Step 5: Final commit if any fixes**

```bash
git add -A
git commit -m "fix: smoke test fixes"
```
