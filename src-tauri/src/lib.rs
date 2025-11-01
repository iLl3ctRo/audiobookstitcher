mod stitcher;

use stitcher::{StitchOptions, StitchResult};
use tauri::Emitter;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![stitch_audiobook_command])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
async fn stitch_audiobook_command(
    window: tauri::Window,
    options: StitchOptions,
) -> Result<StitchResult, String> {
    match stitcher::stitch_audiobook(window.clone(), options).await {
        Ok(result) => Ok(result),
        Err(err) => {
            let _ = window.emit(
                "stitch-progress",
                stitcher::StitchProgress {
                    stage: stitcher::ProgressStage::Failed,
                    current: 0,
                    total: 0,
                    message: Some(err.to_string()),
                },
            );
            Err(err.to_string())
        }
    }
}
