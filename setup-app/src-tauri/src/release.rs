use crate::{hash, platform, AppState};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::sync::{atomic::AtomicBool, mpsc};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Deserialize)]
struct ReleaseMetadata {
    version: String,
    archive: String,
    root: String,
    sha256: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseKit {
    pub ready: bool,
    pub detail: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparationEvent {
    pub kind: &'static str,
    pub message: String,
    pub progress: u8,
    pub output_path: Option<String>,
}

impl PreparationEvent {
    pub fn done(path: PathBuf) -> Self {
        Self {
            kind: "done",
            message: "Package ready".into(),
            progress: 100,
            output_path: Some(path.to_string_lossy().into_owned()),
        }
    }

    pub fn failed(message: String) -> Self {
        Self {
            kind: "error",
            message,
            progress: 0,
            output_path: None,
        }
    }

    pub fn cancelled() -> Self {
        Self {
            kind: "cancelled",
            message: "Preparation cancelled".into(),
            progress: 0,
            output_path: None,
        }
    }
}

pub fn inspect_kit(app: &AppHandle) -> ReleaseKit {
    let metadata = metadata();
    if let Some(root) = release_root_override() {
        let ready = valid_release_root(&root);
        return ReleaseKit {
            ready,
            detail: if ready {
                "Included".into()
            } else {
                "SVMM_RELEASE_ROOT does not contain a release kit".into()
            },
        };
    }
    match find_archive(app) {
        Some(path) => match verify_archive(&path) {
            Ok(()) => ReleaseKit {
                ready: true,
                detail: "Included".into(),
            },
            Err(detail) => ReleaseKit {
                ready: false,
                detail,
            },
        },
        None => ReleaseKit {
            ready: false,
            detail: format!("{} was not found", metadata.archive),
        },
    }
}

pub fn default_output() -> PathBuf {
    dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Stardew Valley for Miyoo Mini")
}

pub fn validate_game(
    app: &AppHandle,
    state: &AppState,
    game_path: PathBuf,
) -> Result<String, String> {
    let root = ensure_release_root(app, state)?;
    let script = root.join("scripts/check-gamefiles.sh");
    let output = platform::script_command(&script, &[&game_path])?
        .output()
        .map_err(|error| format!("Could not run game validation: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if message.is_empty() {
            "Game validation failed".into()
        } else {
            message
        })
    }
}

pub fn ensure_release_root(app: &AppHandle, state: &AppState) -> Result<PathBuf, String> {
    let _guard = state
        .kit_lock
        .lock()
        .map_err(|_| "Release cache lock failed".to_string())?;
    if let Some(root) = release_root_override() {
        return if valid_release_root(&root) {
            Ok(root)
        } else {
            Err("SVMM_RELEASE_ROOT does not contain a complete release kit".into())
        };
    }

    let metadata = metadata();
    let archive = find_archive(app).ok_or_else(|| format!("{} was not found", metadata.archive))?;
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("Could not open the app cache: {error}"))?;
    ensure_release_root_in_cache(&cache, &archive, metadata)
}

pub fn run_preparation(
    app: &AppHandle,
    release_root: &Path,
    game_path: PathBuf,
    output_path: PathBuf,
    cancel: &AtomicBool,
) -> Result<Option<PathBuf>, String> {
    emit(app, "status", "Verifying and preparing game files", 4);
    let script = release_root.join("prepare.sh");
    let mut command = platform::script_command(&script, &[&game_path, &output_path])?;
    platform::configure_process_group(&mut command);
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Could not start preparation: {error}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or("Could not read preparation output")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Could not read preparation errors")?;
    let (sender, receiver) = mpsc::channel::<ProcessOutput>();
    let stderr_sender = sender.clone();
    std::thread::spawn(move || read_lines(stdout, false, sender));
    std::thread::spawn(move || read_lines(stderr, true, stderr_sender));

    let mut progress = 4;
    let mut open_readers: usize = 2;
    let mut status = None;
    let mut cancel_started = None;
    let mut last_error = None;
    while status.is_none() || open_readers > 0 {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(ProcessOutput::Line { is_error, line }) => {
                progress = progress.max(progress_for_line(&line));
                if is_error && !line.trim().is_empty() {
                    last_error = Some(line.clone());
                }
                emit(
                    app,
                    if is_error { "warning" } else { "log" },
                    &line,
                    progress,
                );
            }
            Ok(ProcessOutput::Closed) => open_readers = open_readers.saturating_sub(1),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => open_readers = 0,
        }

        if cancel.load(std::sync::atomic::Ordering::SeqCst) && cancel_started.is_none() {
            emit(app, "status", "Stopping preparation", progress);
            platform::terminate_process_tree(child.id(), false);
            cancel_started = Some(Instant::now());
        }
        if cancel_started
            .map(|started| started.elapsed() > Duration::from_secs(5))
            .unwrap_or(false)
            && status.is_none()
        {
            platform::terminate_process_tree(child.id(), true);
        }
        if status.is_none() {
            status = child
                .try_wait()
                .map_err(|error| format!("Could not check preparation: {error}"))?;
        }
    }

    let status = match status {
        Some(status) => status,
        None => child
            .wait()
            .map_err(|error| format!("Could not finish preparation: {error}"))?,
    };
    if cancel_started.is_some() {
        return Ok(None);
    }
    if status.success() {
        Ok(Some(output_path))
    } else {
        Err(last_error.unwrap_or_else(|| format!("Preparation stopped with {status}")))
    }
}

enum ProcessOutput {
    Line { is_error: bool, line: String },
    Closed,
}

fn read_lines(reader: impl std::io::Read, is_error: bool, sender: mpsc::Sender<ProcessOutput>) {
    for line in BufReader::new(reader).lines().map_while(Result::ok) {
        let _ = sender.send(ProcessOutput::Line { is_error, line });
    }
    let _ = sender.send(ProcessOutput::Closed);
}

fn emit(app: &AppHandle, kind: &'static str, message: &str, progress: u8) {
    let _ = app.emit(
        "preparation-progress",
        PreparationEvent {
            kind,
            message: message.into(),
            progress,
            output_path: None,
        },
    );
}

fn progress_for_line(line: &str) -> u8 {
    let lowercase = line.to_ascii_lowercase();
    if lowercase.contains("game files verified") {
        18
    } else if lowercase.contains("serializer") {
        48
    } else if lowercase.contains("aot") {
        64
    } else if lowercase.contains("texture") || lowercase.contains("bake") {
        82
    } else if lowercase.contains("packageverifier") {
        96
    } else if lowercase.contains("prepared onionos package") {
        99
    } else {
        4
    }
}

fn find_archive(app: &AppHandle) -> Option<PathBuf> {
    let metadata = metadata();
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("SVMM_RELEASE_ARCHIVE") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(resources) = app.path().resource_dir() {
        candidates.push(resources.join("release-kit.tar.gz"));
        candidates.push(resources.join(&metadata.archive));
    }
    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../releases")
            .join(format!("v{}", metadata.version))
            .join(&metadata.archive),
    );
    candidates.into_iter().find(|path| path.is_file())
}

fn release_root_override() -> Option<PathBuf> {
    std::env::var_os("SVMM_RELEASE_ROOT").map(PathBuf::from)
}

fn valid_release_root(root: &Path) -> bool {
    root.join("prepare.sh").is_file()
        && root.join("scripts/check-gamefiles.sh").is_file()
        && root.join("runtime-template").is_dir()
        && root.join("tools/managed/PackageVerifier.exe").is_file()
}

const CACHE_MARKER: &str = ".release-sha256";

fn cache_name(metadata: &ReleaseMetadata) -> Result<String, String> {
    if metadata.sha256.len() != 64 || !metadata.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("release-kit.json contains an invalid SHA-256".into());
    }
    Ok(format!("release-{}-{}", metadata.version, metadata.sha256))
}

fn valid_cached_release(root: &Path, metadata: &ReleaseMetadata) -> bool {
    valid_release_root(root)
        && fs::read_to_string(root.join(CACHE_MARKER))
            .map(|value| value.trim() == metadata.sha256)
            .unwrap_or(false)
}

fn ensure_release_root_in_cache(
    cache: &Path,
    archive: &Path,
    metadata: &ReleaseMetadata,
) -> Result<PathBuf, String> {
    verify_archive_for(archive, metadata)?;
    fs::create_dir_all(cache).map_err(io_error("Could not create the app cache"))?;

    let name = cache_name(metadata)?;
    let root = cache.join(&name);
    if valid_cached_release(&root, metadata) {
        return Ok(root);
    }

    let staging = cache.join(format!("{name}.tmp"));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(io_error("Could not clear the release cache"))?;
    }
    fs::create_dir_all(&staging).map_err(io_error("Could not create the release cache"))?;

    let result: Result<PathBuf, String> = (|| {
        let file =
            fs::File::open(archive).map_err(io_error("Could not open the release archive"))?;
        let mut tar = tar::Archive::new(GzDecoder::new(file));
        tar.unpack(&staging)
            .map_err(io_error("Could not extract the release archive"))?;
        let extracted = staging.join(&metadata.root);
        if !valid_release_root(&extracted) {
            return Err("The release archive is incomplete".into());
        }
        fs::write(
            extracted.join(CACHE_MARKER),
            format!("{}\n", metadata.sha256),
        )
        .map_err(io_error("Could not mark the release cache"))?;

        if root.exists() {
            fs::remove_dir_all(&root).map_err(io_error("Could not replace the release cache"))?;
        }
        fs::rename(&extracted, &root).map_err(io_error("Could not finalize the release cache"))?;
        Ok(root.clone())
    })();

    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(io_error("Could not clean the release cache"))?;
    }
    let root = result?;

    let legacy = cache.join(format!("release-{}", metadata.version));
    if legacy != root && legacy.exists() {
        fs::remove_dir_all(legacy).map_err(io_error("Could not remove the old release cache"))?;
    }
    Ok(root)
}

fn verify_archive(path: &Path) -> Result<(), String> {
    verify_archive_for(path, metadata())
}

fn verify_archive_for(path: &Path, metadata: &ReleaseMetadata) -> Result<(), String> {
    let actual = hash::sha256_file(path)
        .map_err(|error| format!("Could not verify {}: {error}", metadata.archive))?;
    if actual == metadata.sha256 {
        Ok(())
    } else {
        Err(format!("{} failed its integrity check", metadata.archive))
    }
}

fn metadata() -> &'static ReleaseMetadata {
    static METADATA: OnceLock<ReleaseMetadata> = OnceLock::new();
    METADATA.get_or_init(|| {
        serde_json::from_str(include_str!("../release-kit.json"))
            .expect("release-kit.json is invalid")
    })
}

fn io_error(context: &'static str) -> impl FnOnce(std::io::Error) -> String {
    move |error| format!("{context}: {error}")
}

#[cfg(test)]
mod tests {
    use super::{
        cache_name, ensure_release_root_in_cache, metadata, progress_for_line,
        valid_cached_release, valid_release_root, ReleaseMetadata, CACHE_MARKER,
    };
    use flate2::{write::GzEncoder, Compression};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use tempfile::TempDir;

    fn write_release_root(root: &Path, checker: &str) {
        fs::create_dir_all(root.join("scripts")).unwrap();
        fs::create_dir_all(root.join("runtime-template")).unwrap();
        fs::create_dir_all(root.join("tools/managed")).unwrap();
        fs::write(root.join("prepare.sh"), "#!/bin/sh\n").unwrap();
        fs::write(root.join("scripts/check-gamefiles.sh"), checker).unwrap();
        fs::write(root.join("tools/managed/PackageVerifier.exe"), "fixture").unwrap();
    }

    fn write_release_archive(temp: &TempDir) -> (PathBuf, ReleaseMetadata) {
        let source = temp.path().join("source/release-root");
        write_release_root(&source, "new checker");

        let archive = temp.path().join("release.tar.gz");
        let file = fs::File::create(&archive).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = tar::Builder::new(encoder);
        builder.append_dir_all("release-root", &source).unwrap();
        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();

        let sha256 = crate::hash::sha256_file(&archive).unwrap();
        (
            archive,
            ReleaseMetadata {
                version: "1.0.1".into(),
                archive: "release.tar.gz".into(),
                root: "release-root".into(),
                sha256,
            },
        )
    }

    #[test]
    fn maps_preparation_output_to_progress() {
        assert_eq!(progress_for_line("Game files verified: /tmp/game"), 18);
        assert_eq!(progress_for_line("PackageVerifier: OK"), 96);
        assert_eq!(progress_for_line("Prepared OnionOS package: /tmp/out"), 99);
    }

    #[test]
    fn release_cache_identity_includes_the_archive_hash() {
        let first = ReleaseMetadata {
            version: "1.0.1".into(),
            archive: "release.tar.gz".into(),
            root: "release-root".into(),
            sha256: "1".repeat(64),
        };
        let second = ReleaseMetadata {
            sha256: "2".repeat(64),
            ..first.clone()
        };

        assert_ne!(cache_name(&first).unwrap(), cache_name(&second).unwrap());
    }

    #[test]
    fn stale_same_version_cache_is_not_reused() {
        let temp = TempDir::new().unwrap();
        let cache = temp.path().join("cache");
        let legacy = cache.join("release-1.0.1");
        write_release_root(&legacy, "old checker");
        assert!(valid_release_root(&legacy));

        let (archive, metadata) = write_release_archive(&temp);
        let selected = ensure_release_root_in_cache(&cache, &archive, &metadata).unwrap();

        assert_ne!(selected, legacy);
        assert!(!legacy.exists());
        assert!(valid_cached_release(&selected, &metadata));
        assert_eq!(
            fs::read_to_string(selected.join("scripts/check-gamefiles.sh")).unwrap(),
            "new checker"
        );
    }

    #[test]
    fn cache_marker_must_match_the_embedded_archive() {
        let temp = TempDir::new().unwrap();
        let cache = temp.path().join("cache");
        let (archive, metadata) = write_release_archive(&temp);
        let selected = ensure_release_root_in_cache(&cache, &archive, &metadata).unwrap();
        fs::write(selected.join(CACHE_MARKER), format!("{}\n", "0".repeat(64))).unwrap();
        fs::write(selected.join("scripts/check-gamefiles.sh"), "stale checker").unwrap();

        let selected = ensure_release_root_in_cache(&cache, &archive, &metadata).unwrap();

        assert!(valid_cached_release(&selected, &metadata));
        assert_eq!(
            fs::read_to_string(selected.join("scripts/check-gamefiles.sh")).unwrap(),
            "new checker"
        );
    }

    #[test]
    #[ignore = "requires SVMM_TEST_RELEASE_ARCHIVE and SVMM_TEST_GAME_DIR"]
    fn extracted_release_validates_external_game() {
        let archive = PathBuf::from(
            std::env::var_os("SVMM_TEST_RELEASE_ARCHIVE")
                .expect("SVMM_TEST_RELEASE_ARCHIVE is required"),
        );
        let game = PathBuf::from(
            std::env::var_os("SVMM_TEST_GAME_DIR").expect("SVMM_TEST_GAME_DIR is required"),
        );
        let temp = TempDir::new().unwrap();
        let legacy = temp.path().join(format!("release-{}", metadata().version));
        write_release_root(&legacy, "#!/bin/sh\nexit 1\n");

        let selected = ensure_release_root_in_cache(temp.path(), &archive, metadata()).unwrap();
        let output = Command::new("sh")
            .arg(selected.join("scripts/check-gamefiles.sh"))
            .arg(game)
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!legacy.exists());
    }
}
