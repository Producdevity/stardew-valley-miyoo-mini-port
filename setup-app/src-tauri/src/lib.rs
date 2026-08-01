mod depot;
mod game_source;
mod hash;
mod platform;
mod prerequisites;
mod release;
mod steam;

use prerequisites::Requirement;
use release::{PreparationEvent, ReleaseKit};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicBool, Arc, Mutex};
use steam::GameCandidate;
use tauri::{AppHandle, Emitter, State};

pub struct AppState {
    busy: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    kit_lock: Mutex<()>,
    source_lock: Mutex<()>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentInfo {
    game: Option<GameCandidate>,
    requirements: Vec<Requirement>,
    default_output: String,
    release_kit: ReleaseKit,
    platform: &'static str,
}

#[tauri::command]
fn inspect_environment(app: AppHandle) -> EnvironmentInfo {
    EnvironmentInfo {
        game: depot::cached_game(&app).or_else(steam::detect_game),
        requirements: prerequisites::inspect(),
        default_output: release::default_output().to_string_lossy().into_owned(),
        release_kit: release::inspect_kit(&app),
        platform: std::env::consts::OS,
    }
}

#[tauri::command]
fn inspect_game(path: String) -> GameCandidate {
    game_source::inspect(PathBuf::from(path))
}

#[tauri::command]
fn save_log(path: String, contents: String) -> Result<(), String> {
    write_log(Path::new(&path), &contents)
}

fn write_log(path: &Path, contents: &str) -> Result<(), String> {
    if path.is_dir() {
        return Err("Choose a file, not a folder".into());
    }
    if !path.parent().is_some_and(Path::is_dir) {
        return Err("The selected folder does not exist".into());
    }
    std::fs::write(path, contents)
        .map_err(|error| format!("Could not save {}: {error}", path.display()))
}

#[tauri::command]
fn validate_game(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<String, String> {
    let game_path = game_source::resolve(&app, &state, PathBuf::from(path))?;
    release::validate_game(&app, &state, game_path)
}

#[tauri::command]
fn start_preparation(
    app: AppHandle,
    state: State<'_, AppState>,
    game_path: String,
    output_path: String,
) -> Result<(), String> {
    let busy = Arc::clone(&state.busy);
    if busy.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("Preparation is already running".into());
    }
    let cancel = Arc::clone(&state.cancel);
    cancel.store(false, std::sync::atomic::Ordering::SeqCst);
    let setup = (|| {
        let game_path = game_source::resolve(&app, &state, PathBuf::from(game_path))?;
        let release_root = release::ensure_release_root(&app, &state)?;
        Ok::<_, String>((game_path, release_root))
    })();
    let (game_path, release_root) = match setup {
        Ok(paths) => paths,
        Err(error) => {
            busy.store(false, std::sync::atomic::Ordering::SeqCst);
            return Err(error);
        }
    };

    std::thread::spawn(move || {
        let result = release::run_preparation(
            &app,
            &release_root,
            game_path,
            PathBuf::from(output_path),
            &cancel,
        );
        let event = match result {
            Ok(Some(path)) => PreparationEvent::done(path),
            Ok(None) => PreparationEvent::cancelled(),
            Err(message) => PreparationEvent::failed(message),
        };
        let _ = app.emit("preparation-progress", event);
        cancel.store(false, std::sync::atomic::Ordering::SeqCst);
        busy.store(false, std::sync::atomic::Ordering::SeqCst);
    });

    Ok(())
}

#[tauri::command]
fn cancel_preparation(state: State<'_, AppState>) -> Result<(), String> {
    if !state.busy.load(std::sync::atomic::Ordering::SeqCst) {
        return Err("Preparation is not running".into());
    }
    state
        .cancel
        .store(true, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
fn start_steam_download(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let busy = Arc::clone(&state.busy);
    if busy.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("Another setup operation is already running".into());
    }
    let cancel = Arc::clone(&state.cancel);
    cancel.store(false, std::sync::atomic::Ordering::SeqCst);
    std::thread::spawn(move || {
        let event = match depot::download_game(&app, &cancel) {
            Ok(Some(game)) => depot::SteamDownloadEvent {
                kind: "done",
                message: format!("Downloaded {} from Steam", game.detail),
                progress: 100,
                game: Some(game),
            },
            Ok(None) => depot::SteamDownloadEvent::cancelled(),
            Err(message) => depot::SteamDownloadEvent::failed(message),
        };
        let _ = app.emit("steam-download-progress", event);
        cancel.store(false, std::sync::atomic::Ordering::SeqCst);
        busy.store(false, std::sync::atomic::Ordering::SeqCst);
    });
    Ok(())
}

#[tauri::command]
fn cancel_steam_download(state: State<'_, AppState>) -> Result<(), String> {
    cancel_preparation(state)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            busy: Arc::new(AtomicBool::new(false)),
            cancel: Arc::new(AtomicBool::new(false)),
            kit_lock: Mutex::new(()),
            source_lock: Mutex::new(()),
        })
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            inspect_environment,
            inspect_game,
            save_log,
            validate_game,
            start_preparation,
            cancel_preparation,
            start_steam_download,
            cancel_steam_download
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the setup app");
}

#[cfg(test)]
mod tests {
    use super::write_log;

    #[test]
    fn writes_support_log() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("setup-log.txt");

        write_log(&path, "setup failed\n").unwrap();

        assert_eq!(std::fs::read_to_string(path).unwrap(), "setup failed\n");
    }

    #[test]
    fn refuses_to_replace_a_folder() {
        let temp = tempfile::tempdir().unwrap();

        let error = write_log(temp.path(), "setup failed\n").unwrap_err();

        assert_eq!(error, "Choose a file, not a folder");
    }
}
