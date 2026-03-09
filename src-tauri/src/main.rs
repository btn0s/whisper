#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod paste;
mod transcribe;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Emitter, Listener, Manager,
    PhysicalPosition,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

const MODEL_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin";

/// Wrapper to make AudioCapture Send+Sync.
struct SendSyncAudio(audio::AudioCapture);
unsafe impl Send for SendSyncAudio {}
unsafe impl Sync for SendSyncAudio {}

struct AppState {
    audio: Mutex<SendSyncAudio>,
    transcriber: Arc<Mutex<Option<transcribe::Transcriber>>>,
    is_recording: Mutex<bool>,
    model_ready: Mutex<bool>,
}

fn model_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(format!("{}/.whisper", home))
}

fn default_model_path() -> String {
    model_dir().join("ggml-base.en.bin").to_string_lossy().to_string()
}

fn main() {
    let model_path = std::env::var("WHISPER_MODEL").unwrap_or_else(|_| default_model_path());
    let model_exists = std::path::Path::new(&model_path).exists();

    let audio_capture = audio::AudioCapture::new().expect("Failed to initialize audio capture");

    // Load transcriber immediately if model exists, otherwise defer
    let transcriber = if model_exists {
        eprintln!("[whisper] Loading model from {}", model_path);
        match transcribe::Transcriber::new(&model_path) {
            Ok(t) => Some(t),
            Err(e) => {
                eprintln!("[whisper] Failed to load model: {}", e);
                None
            }
        }
    } else {
        None
    };

    let state = AppState {
        audio: Mutex::new(SendSyncAudio(audio_capture)),
        transcriber: Arc::new(Mutex::new(transcriber)),
        is_recording: Mutex::new(false),
        model_ready: Mutex::new(model_exists),
    };

    let toggle_shortcut = Shortcut::new(Some(Modifiers::ALT), Code::Space);
    let escape_shortcut = Shortcut::new(None, Code::Escape);

    tauri::Builder::default()
        .manage(state)
        .setup(move |app| {
            // Hide dock icon — menu bar only
            #[cfg(target_os = "macos")]
            {
                use tauri::ActivationPolicy;
                app.set_activation_policy(ActivationPolicy::Accessory);
            }

            // --- Tray Icon ---
            let quit = MenuItemBuilder::with_id("quit", "Quit whisper").build(app)?;
            let menu = MenuBuilder::new(app).items(&[&quit]).build()?;

            let _tray = TrayIconBuilder::with_id("whisper-tray")
                .tooltip("whisper — Voice to Text")
                .menu(&menu)
                .on_menu_event(|app, event| {
                    if event.id().as_ref() == "quit" {
                        app.exit(0);
                    }
                })
                .build(app)?;

            // --- Global Shortcuts ---
            let app_handle = app.handle().clone();
            app_handle.plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(move |app, shortcut, event| {
                        if event.state != ShortcutState::Pressed {
                            return;
                        }

                        if *shortcut == toggle_shortcut {
                            toggle_recording(app);
                        } else if *shortcut == escape_shortcut {
                            cancel_recording(app);
                        }
                    })
                    .build(),
            )?;

            // Only register toggle globally; Escape is registered/unregistered with recording
            app.global_shortcut().register(toggle_shortcut)?;

            // Listen for frontend button events
            let app_handle_cancel = app.handle().clone();
            app.listen("cancel-recording", move |_| {
                cancel_recording(&app_handle_cancel);
            });

            let app_handle_stop = app.handle().clone();
            app.listen("stop-recording", move |_| {
                stop_and_process(&app_handle_stop);
            });

            if model_exists {
                // Model already loaded — tell frontend to remove setup spinner
                let app_handle = app.handle().clone();
                std::thread::spawn(move || {
                    // Small delay so webview is ready to receive events
                    std::thread::sleep(Duration::from_millis(500));
                    let _ = app_handle.emit("model-download", "ready");
                });
            } else {
                // Show overlay with spinner, download in background
                if let Some(window) = app.get_webview_window("overlay") {
                    if let Some(monitor) = window.primary_monitor().ok().flatten() {
                        let screen = monitor.size();
                        let scale = monitor.scale_factor();
                        let win_w = (160.0 * scale) as i32;
                        let win_h = (40.0 * scale) as i32;
                        let x = (screen.width as i32 - win_w) / 2;
                        let y = screen.height as i32 - win_h - (60.0 * scale) as i32;
                        let _ = window.set_position(PhysicalPosition::new(x, y));
                    }
                    let _ = window.show();
                }
                let app_handle = app.handle().clone();
                let model_path = model_path.clone();
                std::thread::spawn(move || {
                    download_model(&app_handle, &model_path);
                });
            }

            eprintln!("[whisper] Ready. Option+Space to record, Escape to stop.");

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running whisper");
}

fn download_model(app: &tauri::AppHandle, model_path: &str) {
    eprintln!("[whisper] Model not found, downloading...");
    let _ = app.emit("model-download", "starting");

    // Ensure directory exists
    if let Some(parent) = std::path::Path::new(model_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let tmp_path = format!("{}.downloading", model_path);

    // Use curl for download with progress — simpler than pulling in reqwest
    let output = std::process::Command::new("curl")
        .args(["-L", "-o", &tmp_path, MODEL_URL])
        .output();

    match output {
        Ok(result) if result.status.success() => {
            // Rename temp file to final path
            if let Err(e) = std::fs::rename(&tmp_path, model_path) {
                eprintln!("[whisper] Failed to move model file: {}", e);
                let _ = app.emit("model-download", "error");
                return;
            }

            eprintln!("[whisper] Model downloaded, loading...");

            // Load the transcriber
            match transcribe::Transcriber::new(model_path) {
                Ok(t) => {
                    let state = app.state::<AppState>();
                    *state.transcriber.lock().unwrap() = Some(t);
                    *state.model_ready.lock().unwrap() = true;
                    eprintln!("[whisper] Model loaded and ready");
                    let _ = app.emit("model-download", "ready");
                    std::thread::sleep(Duration::from_millis(300));
                    hide_overlay(app);
                }
                Err(e) => {
                    eprintln!("[whisper] Failed to load downloaded model: {}", e);
                    let _ = app.emit("model-download", "error");
                }
            }
        }
        Ok(result) => {
            let _ = std::fs::remove_file(&tmp_path);
            eprintln!("[whisper] Download failed: {}", String::from_utf8_lossy(&result.stderr));
            let _ = app.emit("model-download", "error");
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp_path);
            eprintln!("[whisper] Download error: {}", e);
            let _ = app.emit("model-download", "error");
        }
    }
}

fn toggle_recording(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let state = state.inner();

    // Block recording if model isn't ready
    if !*state.model_ready.lock().unwrap() {
        eprintln!("[whisper] Model not ready yet, ignoring record request");
        return;
    }

    let mut is_recording = state.is_recording.lock().unwrap();

    if *is_recording {
        drop(is_recording);
        stop_and_process(app);
    } else {
        let mut audio = state.audio.lock().unwrap();
        if let Err(e) = audio.0.start() {
            eprintln!("[whisper] Failed to start recording: {}", e);
            return;
        }
        *is_recording = true;
        play_sound("dictation-start.wav");
        eprintln!("[whisper] Recording started");

        // Register Escape shortcut only while recording (spawned to avoid main-thread deadlock)
        let app_clone = app.clone();
        std::thread::spawn(move || {
            let esc = Shortcut::new(None, Code::Escape);
            match app_clone.global_shortcut().register(esc) {
                Ok(_) => eprintln!("[whisper] Escape shortcut registered"),
                Err(e) => eprintln!("[whisper] Escape register failed: {}", e),
            }
        });

        if let Some(window) = app.get_webview_window("overlay") {
            // Position bottom-center of primary monitor
            if let Some(monitor) = window.primary_monitor().ok().flatten() {
                let screen = monitor.size();
                let scale = monitor.scale_factor();
                let win_w = (160.0 * scale) as i32;
                let win_h = (40.0 * scale) as i32;
                let x = (screen.width as i32 - win_w) / 2;
                let y = screen.height as i32 - win_h - (60.0 * scale) as i32;
                let _ = window.set_position(PhysicalPosition::new(x, y));
            }
            let _ = window.show();
            let _ = app.emit("recording-state", "recording");
        }

        // Audio level emitter for waveform UI
        let app_handle_levels = app.clone();
        std::thread::spawn(move || {
            audio_level_loop(&app_handle_levels);
        });

        // Live transcription: only transcribe the last ~5 seconds of audio
        let app_handle = app.clone();
        let transcriber = state.transcriber.clone();
        std::thread::spawn(move || {
            live_transcription_loop(&app_handle, &transcriber);
        });
    }
}

fn sounds_dir() -> PathBuf {
    // In dev: src-tauri/sounds/, in prod: bundled resource
    let exe = std::env::current_exe().unwrap_or_default();
    let dev_path = exe.parent().unwrap_or(std::path::Path::new("."))
        .join("../../../src-tauri/sounds");
    if dev_path.exists() {
        return dev_path;
    }
    exe.parent().unwrap_or(std::path::Path::new(".")).join("sounds")
}

fn play_sound(name: &str) {
    let path = sounds_dir().join(name);
    if path.exists() {
        std::thread::spawn(move || {
            let _ = std::process::Command::new("afplay")
                .arg(&path)
                .spawn();
        });
    }
}

fn cancel_recording(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let state = state.inner();
    let mut is_recording = state.is_recording.lock().unwrap();
    if !*is_recording {
        return;
    }
    *is_recording = false;
    drop(is_recording);

    let mut audio = state.audio.lock().unwrap();
    audio.0.stop();
    drop(audio);

    // Unregister Escape so it doesn't block other apps (spawned to avoid main-thread deadlock)
    let app_clone = app.clone();
    std::thread::spawn(move || {
        let esc = Shortcut::new(None, Code::Escape);
        match app_clone.global_shortcut().unregister(esc) {
            Ok(_) => eprintln!("[whisper] Escape shortcut unregistered"),
            Err(e) => eprintln!("[whisper] Escape unregister failed: {}", e),
        }
    });

    play_sound("cancel.wav");
    eprintln!("[whisper] Recording cancelled");
    hide_overlay(app);
}

fn audio_level_loop(app: &tauri::AppHandle) {
    loop {
        std::thread::sleep(Duration::from_millis(50)); // 20fps

        let state = app.state::<AppState>();
        let state = state.inner();
        let is_recording = state.is_recording.lock().unwrap();
        if !*is_recording {
            break;
        }
        drop(is_recording);

        let audio = state.audio.lock().unwrap();
        let levels = audio.0.levels();
        drop(audio);

        let _ = app.emit("audio-levels", &levels);
    }
}

fn live_transcription_loop(
    app: &tauri::AppHandle,
    transcriber: &Arc<Mutex<Option<transcribe::Transcriber>>>,
) {
    const WHISPER_RATE: usize = 16_000;
    const WINDOW_SECS: usize = 5;
    const WINDOW_SAMPLES: usize = WHISPER_RATE * WINDOW_SECS;

    loop {
        std::thread::sleep(Duration::from_secs(3));

        let state = app.state::<AppState>();
        let state = state.inner();
        let is_recording = state.is_recording.lock().unwrap();
        if !*is_recording {
            break;
        }
        drop(is_recording);

        // Take a snapshot quickly and release the lock
        let snapshot = {
            let audio = state.audio.lock().unwrap();
            audio.0.snapshot()
        };

        if snapshot.len() < WHISPER_RATE {
            // Less than 1 second of audio, skip
            continue;
        }

        // Only transcribe the last N seconds to keep it fast
        let window = if snapshot.len() > WINDOW_SAMPLES {
            &snapshot[snapshot.len() - WINDOW_SAMPLES..]
        } else {
            &snapshot
        };

        eprintln!("[whisper] Live transcribing {} samples ({:.1}s)...",
            window.len(), window.len() as f64 / WHISPER_RATE as f64);

        let guard = transcriber.lock().unwrap();
        if let Some(ref t) = *guard {
            match t.transcribe(window) {
                Ok(text) if !text.is_empty() => {
                    eprintln!("[whisper] Live: \"{}\"", &text);
                    let _ = app.emit("transcription-update", &text);
                }
                Err(e) => eprintln!("[whisper] Live transcription error: {}", e),
                _ => {
                    eprintln!("[whisper] Live: (no speech detected)");
                }
            }
        }
    }
}

fn stop_and_process(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let state = state.inner();

    {
        let mut is_recording = state.is_recording.lock().unwrap();
        if !*is_recording {
            return;
        }
        *is_recording = false;
    }

    // Unregister Escape so it doesn't block other apps (spawned to avoid main-thread deadlock)
    let app_clone = app.clone();
    std::thread::spawn(move || {
        let esc = Shortcut::new(None, Code::Escape);
        match app_clone.global_shortcut().unregister(esc) {
            Ok(_) => eprintln!("[whisper] Escape shortcut unregistered"),
            Err(e) => eprintln!("[whisper] Escape unregister failed: {}", e),
        }
    });

    play_sound("dictation-stop.wav");
    eprintln!("[whisper] Recording stopped, processing...");
    let _ = app.emit("recording-state", "processing");

    // Stop audio and get all samples
    let samples = {
        let mut audio = state.audio.lock().unwrap();
        audio.0.stop()
    };

    eprintln!("[whisper] Got {} samples ({:.1}s) for final transcription",
        samples.len(), samples.len() as f64 / 16_000.0);

    let app_handle = app.clone();
    let transcriber = state.transcriber.clone();
    tauri::async_runtime::spawn(async move {
        // Final transcription
        let raw_text = {
            let guard = transcriber.lock().unwrap();
            match guard.as_ref() {
                Some(t) => match t.transcribe(&samples) {
                    Ok(text) => text,
                    Err(e) => {
                        eprintln!("[whisper] Transcription error: {}", e);
                        hide_overlay(&app_handle);
                        return;
                    }
                },
                None => {
                    eprintln!("[whisper] Transcriber not available");
                    hide_overlay(&app_handle);
                    return;
                }
            }
        };

        eprintln!("[whisper] Raw transcript: \"{}\"", &raw_text);

        if raw_text.is_empty() {
            eprintln!("[whisper] Empty transcript, skipping");
            hide_overlay(&app_handle);
            return;
        }

        let _ = app_handle.emit("transcription-update", &raw_text);

        // Hide overlay first so focus returns to the user's app
        hide_overlay(&app_handle);
        tokio::time::sleep(Duration::from_millis(100)).await;

        play_sound("paste.wav");
        if let Err(e) = paste::paste_text(&raw_text) {
            eprintln!("[whisper] Paste error: {}", e);
        }

        eprintln!("[whisper] Done. Ready for next recording.");
    });
}

fn hide_overlay(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.hide();
    }
    let _ = app.emit("recording-state", "idle");
    let _ = app.emit("transcription-update", "");
}
