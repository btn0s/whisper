#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod context;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .run(tauri::generate_context!())
        .expect("error while running whispr");
}
