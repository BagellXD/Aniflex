mod prediction;
pub mod ani_cli;
pub mod video_server;

use std::{
    fs,
    path::PathBuf,
};

#[tauri::command]
fn get_library() -> Result<Vec<String>, String> {
    let media_dir =
        PathBuf::from(
            env!("CARGO_MANIFEST_DIR"),
        )
        .join("media");

    if !media_dir.exists() {
        return Ok(Vec::new());
    }

    let mut videos =
        Vec::new();

    fn scan_directory(
        directory: &PathBuf,
        videos: &mut Vec<String>,
    ) -> Result<(), String> {
        let entries =
            fs::read_dir(directory)
                .map_err(|error| {
                    error.to_string()
                })?;

        for entry in entries {
            let entry =
                entry.map_err(|error| {
                    error.to_string()
                })?;

            let path =
                entry.path();

            if path.is_dir() {
                scan_directory(
                    &path,
                    videos,
                )?;

                continue;
            }

            let Some(extension) =
                path.extension()
                    .and_then(|ext| {
                        ext.to_str()
                    })
            else {
                continue;
            };

            match extension
                .to_lowercase()
                .as_str()
            {
                "mp4"
                | "mkv"
                | "webm"
                | "avi"
                | "mov" => {
                    videos.push(
                        path.to_string_lossy()
                            .to_string(),
                    );
                }

                _ => {}
            }
        }

        Ok(())
    }

    scan_directory(
        &media_dir,
        &mut videos,
    )?;

    Ok(videos)
}

#[cfg_attr(
    mobile,
    tauri::mobile_entry_point
)]
pub fn run() {
    tauri::Builder::default()
        .setup(|_app| {
            video_server::start();
            Ok(())
        })
        .invoke_handler(
            tauri::generate_handler![
                get_library
            ],
        )
        .run(
            tauri::generate_context!(),
        )
        .expect(
            "failed to run Orion Player",
        );
}