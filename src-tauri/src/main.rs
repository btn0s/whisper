#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod context;
mod llm;
mod paste;
mod transcribe;

use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Emitter, Manager,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// Wrapper to make AudioCapture Send+Sync.
/// Safety: AudioCapture is only accessed behind a Mutex, so only one thread
/// touches the cpal::Stream at a time, and we only use it from the main process.
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
        std::env::var("WHISPR_LLM_MODEL").unwrap_or_else(|_| "qwen3.5:8b".to_string());

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

    let toggle_shortcut = Shortcut::new(Some(Modifiers::META | Modifiers::SHIFT), Code::Space);
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
    let state = state.inner();
    let mut is_recording = state.is_recording.lock().unwrap();

    if *is_recording {
        drop(is_recording);
        stop_and_process(app);
    } else {
        // Start recording
        let mut audio = state.audio.lock().unwrap();
        if let Err(e) = audio.0.start() {
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
        let state = state.inner();
        let is_recording = state.is_recording.lock().unwrap();
        if !*is_recording {
            break;
        }
        drop(is_recording);

        let audio = state.audio.lock().unwrap();
        let snapshot = audio.0.snapshot();
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
    let state = state.inner();
    let mut is_recording = state.is_recording.lock().unwrap();
    if !*is_recording {
        return;
    }
    *is_recording = false;
    drop(is_recording);

    let _ = app.emit("recording-state", "processing");

    // Stop audio and get samples
    let mut audio = state.audio.lock().unwrap();
    let samples = audio.0.stop();
    drop(audio);

    // Final transcription + LLM reformat (async)
    let app_handle = app.clone();
    let transcriber = state.transcriber.clone();
    tauri::async_runtime::spawn(async move {
        let state = app_handle.state::<AppState>();
        let state = state.inner();

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
        let file_context =
            context::detect_project_root().and_then(|root| context::collect_file_tree(&root));

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
