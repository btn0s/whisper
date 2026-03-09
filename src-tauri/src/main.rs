#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod context;
mod llm;
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

/// Wrapper to make AudioCapture Send+Sync.
struct SendSyncAudio(audio::AudioCapture);
unsafe impl Send for SendSyncAudio {}
unsafe impl Sync for SendSyncAudio {}

struct AppState {
    audio: Mutex<SendSyncAudio>,
    transcriber: Arc<transcribe::Transcriber>,
    llm: llm::LlmClient,
    is_recording: Mutex<bool>,
}

fn main() {
    let model_path = std::env::var("WHISPR_MODEL").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{}/.whispr/ggml-base.en.bin", home)
    });

    let qwen_model =
        std::env::var("WHISPR_LLM_MODEL").unwrap_or_else(|_| "qwen3.5".to_string());

    let transcriber = Arc::new(
        transcribe::Transcriber::new(&model_path).expect(
            "Failed to load Whisper model. Set WHISPR_MODEL env var or place model at ~/.whispr/ggml-base.en.bin",
        ),
    );

    let audio_capture = audio::AudioCapture::new().expect("Failed to initialize audio capture");

    let state = AppState {
        audio: Mutex::new(SendSyncAudio(audio_capture)),
        transcriber,
        llm: llm::LlmClient::new(&qwen_model),
        is_recording: Mutex::new(false),
    };

    let toggle_shortcut = Shortcut::new(Some(Modifiers::ALT), Code::Space);
    let escape_shortcut = Shortcut::new(None, Code::Escape);

    tauri::Builder::default()
        .manage(state)
        .setup(move |app| {
            // --- Tray Icon ---
            let quit = MenuItemBuilder::with_id("quit", "Quit Whispr").build(app)?;
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

            eprintln!("[whispr] Ready. Option+Space to record, Escape to stop.");

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running whispr");
}

fn toggle_recording(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let state = state.inner();
    let mut is_recording = state.is_recording.lock().unwrap();

    if *is_recording {
        drop(is_recording);
        stop_and_process(app);
    } else {
        let mut audio = state.audio.lock().unwrap();
        if let Err(e) = audio.0.start() {
            eprintln!("[whispr] Failed to start recording: {}", e);
            return;
        }
        *is_recording = true;
        play_sound("dictation-start.wav");
        eprintln!("[whispr] Recording started");

        // Register Escape shortcut only while recording (spawned to avoid main-thread deadlock)
        let app_clone = app.clone();
        std::thread::spawn(move || {
            let esc = Shortcut::new(None, Code::Escape);
            match app_clone.global_shortcut().register(esc) {
                Ok(_) => eprintln!("[whispr] Escape shortcut registered"),
                Err(e) => eprintln!("[whispr] Escape register failed: {}", e),
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
            Ok(_) => eprintln!("[whispr] Escape shortcut unregistered"),
            Err(e) => eprintln!("[whispr] Escape unregister failed: {}", e),
        }
    });

    play_sound("cancel.wav");
    eprintln!("[whispr] Recording cancelled");
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

fn live_transcription_loop(app: &tauri::AppHandle, transcriber: &transcribe::Transcriber) {
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

        eprintln!("[whispr] Live transcribing {} samples ({:.1}s)...",
            window.len(), window.len() as f64 / WHISPER_RATE as f64);

        match transcriber.transcribe(window) {
            Ok(text) if !text.is_empty() => {
                eprintln!("[whispr] Live: \"{}\"", &text);
                let _ = app.emit("transcription-update", &text);
            }
            Err(e) => eprintln!("[whispr] Live transcription error: {}", e),
            _ => {
                eprintln!("[whispr] Live: (no speech detected)");
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
            Ok(_) => eprintln!("[whispr] Escape shortcut unregistered"),
            Err(e) => eprintln!("[whispr] Escape unregister failed: {}", e),
        }
    });

    play_sound("dictation-stop.wav");
    eprintln!("[whispr] Recording stopped, processing...");
    let _ = app.emit("recording-state", "processing");

    // Stop audio and get all samples
    let samples = {
        let mut audio = state.audio.lock().unwrap();
        audio.0.stop()
    };

    eprintln!("[whispr] Got {} samples ({:.1}s) for final transcription",
        samples.len(), samples.len() as f64 / 16_000.0);

    let app_handle = app.clone();
    let transcriber = state.transcriber.clone();
    tauri::async_runtime::spawn(async move {
        let state = app_handle.state::<AppState>();
        let state = state.inner();

        // Final transcription
        let raw_text = match transcriber.transcribe(&samples) {
            Ok(text) => text,
            Err(e) => {
                eprintln!("[whispr] Transcription error: {}", e);
                hide_overlay(&app_handle);
                return;
            }
        };

        eprintln!("[whispr] Raw transcript: \"{}\"", &raw_text);

        if raw_text.is_empty() {
            eprintln!("[whispr] Empty transcript, skipping");
            hide_overlay(&app_handle);
            return;
        }

        let _ = app_handle.emit("transcription-update", &raw_text);

        // TODO: LLM reformatting via Ollama (disabled for now)
        // let file_context = context::detect_project_root()
        //     .and_then(|root| context::collect_file_tree(&root));
        // let final_text = state.llm.reformat(&raw_text, file_context.as_deref()).await
        //     .unwrap_or(raw_text);

        // Hide overlay first so focus returns to the user's app
        hide_overlay(&app_handle);
        tokio::time::sleep(Duration::from_millis(100)).await;

        play_sound("paste.wav");
        if let Err(e) = paste::paste_text(&raw_text) {
            eprintln!("[whispr] Paste error: {}", e);
        }

        eprintln!("[whispr] Done. Ready for next recording.");
    });
}

fn hide_overlay(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.hide();
    }
    let _ = app.emit("recording-state", "idle");
    let _ = app.emit("transcription-update", "");
}
