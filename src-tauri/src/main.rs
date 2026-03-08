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

    let toggle_shortcut = Shortcut::new(Some(Modifiers::ALT | Modifiers::SHIFT), Code::Space);
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

            eprintln!("[whispr] Ready. Option+Shift+Space to record, Escape to stop.");

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
        eprintln!("[whispr] Recording started");

        if let Some(window) = app.get_webview_window("overlay") {
            let _ = window.show();
            // Don't take focus — keep the user's app focused for paste
            let _ = app.emit("recording-state", "recording");
        }

        // Live transcription: only transcribe the last ~5 seconds of audio
        let app_handle = app.clone();
        let transcriber = state.transcriber.clone();
        std::thread::spawn(move || {
            live_transcription_loop(&app_handle, &transcriber);
        });
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
