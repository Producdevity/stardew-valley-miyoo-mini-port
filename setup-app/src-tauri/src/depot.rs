use crate::steam::{self, GameCandidate};
use crate::{hash, platform};
use serde::Serialize;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use zip::ZipArchive;

const VERSION: &str = "3.4.0";
const TAG: &str = "DepotDownloader_3.4.0";
const APP_ID: &str = "413150";
const DEPOT_ID: &str = "413151";
const LOGIN_ID: &str = "413150";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamDownloadEvent {
    pub kind: &'static str,
    pub message: String,
    pub progress: u8,
    pub game: Option<GameCandidate>,
}

impl SteamDownloadEvent {
    pub fn failed(message: String) -> Self {
        Self {
            kind: "error",
            message,
            progress: 0,
            game: None,
        }
    }

    pub fn cancelled() -> Self {
        Self {
            kind: "cancelled",
            message: "Steam download cancelled".into(),
            progress: 0,
            game: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ToolSpec {
    archive: &'static str,
    sha256: &'static str,
    executable: &'static str,
    executable_sha256: &'static str,
}

pub fn cached_game(app: &AppHandle) -> Option<GameCandidate> {
    let game_dir = app
        .path()
        .app_cache_dir()
        .ok()?
        .join("steam")
        .join("stardew-compatibility");
    let candidate = steam::inspect_game(game_dir, "Steam download");
    candidate.supported.then_some(candidate)
}

pub fn download_game(
    app: &AppHandle,
    cancel: &AtomicBool,
) -> Result<Option<GameCandidate>, String> {
    emit(app, "status", "Getting DepotDownloader", 2, None);
    let tool = match ensure_tool(app, cancel)? {
        Some(tool) => tool,
        None => return Ok(None),
    };

    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("Could not open the app cache: {error}"))?;
    let steam_dir = cache.join("steam");
    let game_dir = steam_dir.join("stardew-compatibility");
    let download_dir = steam_dir.join("stardew-compatibility.download");
    let session_dir = steam_dir.join("login-session");
    fs::create_dir_all(&steam_dir)
        .map_err(io_error("Could not create the Steam download folder"))?;
    if download_dir.exists() {
        fs::remove_dir_all(&download_dir)
            .map_err(io_error("Could not clear the previous Steam download"))?;
    }
    fs::create_dir_all(&download_dir)
        .map_err(io_error("Could not create the Steam download folder"))?;
    if session_dir.exists() {
        fs::remove_dir_all(&session_dir)
            .map_err(io_error("Could not clear the previous Steam login session"))?;
    }
    fs::create_dir_all(&session_dir)
        .map_err(io_error("Could not create the Steam login session"))?;

    emit(
        app,
        "status",
        "Scan the QR code with Steam Mobile",
        12,
        None,
    );
    let result = run_depot_downloader(app, cancel, &tool, &session_dir, &download_dir);
    let _ = fs::remove_dir_all(&session_dir);
    match result {
        Ok(Some(())) => {}
        Ok(None) => {
            let _ = fs::remove_dir_all(&download_dir);
            return Ok(None);
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&download_dir);
            return Err(error);
        }
    }

    emit(app, "status", "Checking the downloaded game", 94, None);
    let candidate = steam::inspect_game(download_dir.clone(), "Steam download");
    if !candidate.supported {
        let _ = fs::remove_dir_all(&download_dir);
        return Err(format!(
            "Steam downloaded a build this release does not support: {}",
            candidate.detail
        ));
    }
    replace_directory(&download_dir, &game_dir)?;
    Ok(Some(steam::inspect_game(game_dir, "Steam download")))
}

fn replace_directory(source: &Path, destination: &Path) -> Result<(), String> {
    let backup = destination.with_extension("previous");
    if backup.exists() {
        fs::remove_dir_all(&backup).map_err(io_error("Could not clear the old Steam download"))?;
    }
    if destination.exists() {
        fs::rename(destination, &backup)
            .map_err(io_error("Could not preserve the previous Steam download"))?;
    }
    if let Err(error) = fs::rename(source, destination) {
        if backup.exists() {
            let _ = fs::rename(&backup, destination);
        }
        return Err(format!("Could not finalize the Steam download: {error}"));
    }
    if backup.exists() {
        fs::remove_dir_all(&backup).map_err(io_error("Could not clear the old Steam download"))?;
    }
    Ok(())
}

fn ensure_tool(app: &AppHandle, cancel: &AtomicBool) -> Result<Option<PathBuf>, String> {
    let spec = current_tool_spec()?;
    if let Some(root) = std::env::var_os("SVMM_DEPOT_DOWNLOADER_ROOT") {
        let executable = PathBuf::from(root).join(spec.executable);
        return if executable.is_file() {
            Ok(Some(executable))
        } else {
            Err("SVMM_DEPOT_DOWNLOADER_ROOT does not contain DepotDownloader".into())
        };
    }

    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("Could not open the app cache: {error}"))?
        .join("tools")
        .join(format!("depotdownloader-{VERSION}"));
    let executable = cache.join(spec.executable);
    let marker = cache.join("archive.sha256");
    if executable.is_file()
        && fs::read_to_string(&marker).ok().as_deref() == Some(spec.sha256)
        && hash::sha256_file(&executable).ok().as_deref() == Some(spec.executable_sha256)
    {
        return Ok(Some(executable));
    }

    let archive = if let Some(path) = std::env::var_os("SVMM_DEPOT_DOWNLOADER_ARCHIVE") {
        PathBuf::from(path)
    } else {
        fs::create_dir_all(cache.parent().expect("tool cache has a parent"))
            .map_err(io_error("Could not create the tool cache"))?;
        let archive = cache.with_extension("zip");
        if !archive.is_file() || hash::sha256_file(&archive).ok().as_deref() != Some(spec.sha256) {
            match download_archive(app, cancel, spec, &archive)? {
                Some(()) => {}
                None => return Ok(None),
            }
        }
        archive
    };

    let actual = hash::sha256_file(&archive)
        .map_err(|error| format!("Could not verify {}: {error}", spec.archive))?;
    if actual != spec.sha256 {
        return Err(format!("{} failed its integrity check", spec.archive));
    }

    let staging = cache.with_extension("tmp");
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(io_error("Could not clear the tool cache"))?;
    }
    fs::create_dir_all(&staging).map_err(io_error("Could not create the tool cache"))?;
    extract_tool(&archive, &staging, spec)?;
    let extracted_hash = hash::sha256_file(&staging.join(spec.executable))
        .map_err(|error| format!("Could not verify DepotDownloader: {error}"))?;
    if extracted_hash != spec.executable_sha256 {
        return Err("DepotDownloader executable failed its integrity check".into());
    }
    fs::write(staging.join("archive.sha256"), spec.sha256)
        .map_err(io_error("Could not finalize the tool cache"))?;
    if cache.exists() {
        fs::remove_dir_all(&cache).map_err(io_error("Could not replace the tool cache"))?;
    }
    fs::rename(&staging, &cache).map_err(io_error("Could not finalize the tool cache"))?;
    emit(app, "status", "DepotDownloader is ready", 10, None);
    Ok(Some(executable))
}

fn download_archive(
    app: &AppHandle,
    cancel: &AtomicBool,
    spec: ToolSpec,
    destination: &Path,
) -> Result<Option<()>, String> {
    let part = destination.with_extension("zip.part");
    if part.exists() {
        fs::remove_file(&part).map_err(io_error("Could not clear the tool download"))?;
    }
    let url = format!(
        "https://github.com/SteamRE/DepotDownloader/releases/download/{TAG}/{}",
        spec.archive
    );
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("stardew-miyoo-setup/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| format!("Could not initialize the downloader: {error}"))?;
    let mut response = client
        .get(url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| format!("Could not download DepotDownloader: {error}"))?;
    let total = response.content_length();
    let mut file = fs::File::create(&part).map_err(io_error("Could not save DepotDownloader"))?;
    let mut buffer = [0_u8; 64 * 1024];
    let mut written = 0_u64;
    loop {
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            drop(file);
            let _ = fs::remove_file(&part);
            return Ok(None);
        }
        let count = response
            .read(&mut buffer)
            .map_err(|error| format!("DepotDownloader download stopped: {error}"))?;
        if count == 0 {
            break;
        }
        file.write_all(&buffer[..count])
            .map_err(io_error("Could not save DepotDownloader"))?;
        written += count as u64;
        let progress = total
            .filter(|size| *size > 0)
            .map(|size| ((written.saturating_mul(7) / size).min(7) + 2) as u8)
            .unwrap_or(5);
        emit(app, "status", "Downloading DepotDownloader", progress, None);
    }
    file.sync_all()
        .map_err(io_error("Could not finish the DepotDownloader download"))?;
    fs::rename(&part, destination)
        .map_err(io_error("Could not finalize the DepotDownloader download"))?;
    Ok(Some(()))
}

fn extract_tool(archive: &Path, destination: &Path, spec: ToolSpec) -> Result<(), String> {
    let file = fs::File::open(archive).map_err(io_error("Could not open DepotDownloader"))?;
    let mut zip = ZipArchive::new(file)
        .map_err(|error| format!("DepotDownloader archive is invalid: {error}"))?;
    let mut executable_found = false;
    let mut license_found = false;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| format!("Could not read DepotDownloader: {error}"))?;
        let path = entry
            .enclosed_name()
            .ok_or("DepotDownloader archive contains an unsafe path")?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("DepotDownloader archive contains an unsafe file name")?
            .to_owned();
        if name != spec.executable && name != "LICENSE" {
            return Err(format!(
                "DepotDownloader archive contains an unexpected file: {name}"
            ));
        }
        let target = destination.join(&name);
        let mut output =
            fs::File::create(&target).map_err(io_error("Could not extract DepotDownloader"))?;
        std::io::copy(&mut entry, &mut output)
            .map_err(io_error("Could not extract DepotDownloader"))?;
        executable_found |= name == spec.executable;
        license_found |= name == "LICENSE";
    }
    if !executable_found || !license_found {
        return Err("DepotDownloader archive is incomplete".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            destination.join(spec.executable),
            fs::Permissions::from_mode(0o755),
        )
        .map_err(io_error("Could not make DepotDownloader executable"))?;
    }
    Ok(())
}

fn run_depot_downloader(
    app: &AppHandle,
    cancel: &AtomicBool,
    executable: &Path,
    session_dir: &Path,
    game_dir: &Path,
) -> Result<Option<()>, String> {
    let mut command = std::process::Command::new(executable);
    command
        .args([
            "-app",
            APP_ID,
            "-depot",
            DEPOT_ID,
            "-branch",
            "compatibility",
            "-os",
            "windows",
            "-osarch",
            "64",
            "-qr",
            "-validate",
            "-loginid",
            LOGIN_ID,
            "-dir",
        ])
        .arg(game_dir)
        .current_dir(session_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    platform::configure_process_group(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not start the Steam download: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Could not read the Steam download")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Could not read Steam download errors")?;
    let (sender, receiver) = mpsc::channel::<ProcessOutput>();
    let stderr_sender = sender.clone();
    std::thread::spawn(move || read_lines(stdout, false, sender));
    std::thread::spawn(move || read_lines(stderr, true, stderr_sender));

    let mut progress = 12;
    let mut readers = 2_usize;
    let mut status = None;
    let mut cancel_started = None;
    let mut last_error = None;
    while status.is_none() || readers > 0 {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(ProcessOutput::Line { is_error, line }) => {
                progress = progress.max(progress_for_line(&line));
                if is_error && !line.trim().is_empty() {
                    last_error = Some(line.clone());
                }
                let kind = output_kind(&line, is_error);
                emit(app, kind, &line, progress, None);
            }
            Ok(ProcessOutput::Closed) => readers = readers.saturating_sub(1),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => readers = 0,
        }
        if cancel.load(std::sync::atomic::Ordering::SeqCst) && cancel_started.is_none() {
            emit(app, "status", "Stopping the Steam download", progress, None);
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
                .map_err(|error| format!("Could not check the Steam download: {error}"))?;
        }
    }
    let status = status.ok_or("Steam download status was lost")?;
    if cancel_started.is_some() {
        return Ok(None);
    }
    if status.success() {
        Ok(Some(()))
    } else {
        Err(last_error.unwrap_or_else(|| format!("Steam download stopped with {status}")))
    }
}

enum ProcessOutput {
    Line { is_error: bool, line: String },
    Closed,
}

fn read_lines(reader: impl Read, is_error: bool, sender: mpsc::Sender<ProcessOutput>) {
    let mut reader = BufReader::new(reader);
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        let read = match reader.read_until(b'\n', &mut bytes) {
            Ok(read) => read,
            Err(_) => break,
        };
        if read == 0 {
            break;
        }
        let line = decode_output_line(&bytes);
        let _ = sender.send(ProcessOutput::Line {
            is_error,
            line: strip_ansi(&line),
        });
    }
    let _ = sender.send(ProcessOutput::Closed);
}

fn decode_output_line(bytes: &[u8]) -> String {
    let mut bytes = bytes;
    if let Some(line) = bytes.strip_suffix(b"\n") {
        bytes = line;
    }
    if let Some(line) = bytes.strip_suffix(b"\r") {
        bytes = line;
    }
    if let Ok(line) = std::str::from_utf8(bytes) {
        return line.to_owned();
    }

    bytes
        .iter()
        .map(|byte| match byte {
            0xdb => '█',
            0x20..=0x7e => char::from(*byte),
            _ => '�',
        })
        .collect()
}

fn strip_ansi(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut result = String::with_capacity(line.len());
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != 0x1b {
            index += 1;
            continue;
        }
        result.push_str(&line[start..index]);
        index += 1;
        if bytes.get(index) == Some(&b'[') {
            index += 1;
            while index < bytes.len() {
                let byte = bytes[index];
                index += 1;
                if (0x40..=0x7e).contains(&byte) {
                    break;
                }
            }
        } else if index < bytes.len() {
            index += 1;
        }
        start = index;
    }
    result.push_str(&line[start..]);
    result
}

fn emit(
    app: &AppHandle,
    kind: &'static str,
    message: &str,
    progress: u8,
    game: Option<GameCandidate>,
) {
    let _ = app.emit(
        "steam-download-progress",
        SteamDownloadEvent {
            kind,
            message: message.into(),
            progress,
            game,
        },
    );
}

fn progress_for_line(line: &str) -> u8 {
    if line.contains("Steam Mobile App") {
        return 16;
    }
    if line.contains("Got session token") || line.contains("Using app branch") {
        return 24;
    }
    if line.contains("Downloading depot") {
        return 30;
    }
    if line.contains("Depot download complete") || line.contains("Total downloaded") {
        return 92;
    }
    percent_in_line(line)
        .map(|percent| 30 + (percent.min(100) * 60 / 100) as u8)
        .unwrap_or(12)
}

fn output_kind(line: &str, is_error: bool) -> &'static str {
    if line.contains("Steam Mobile App") {
        "qr-start"
    } else if is_qr_row(line) {
        "qr"
    } else if is_error {
        "warning"
    } else {
        "log"
    }
}

fn is_qr_row(line: &str) -> bool {
    line.chars().count() >= 16
        && line
            .chars()
            .all(|character| character == ' ' || character == '█')
}

fn percent_in_line(line: &str) -> Option<u32> {
    let percent = line.find('%')?;
    let prefix = &line[..percent];
    let token = prefix
        .rsplit(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
        .next()?;
    token.parse::<f64>().ok().map(|value| value as u32)
}

fn current_tool_spec() -> Result<ToolSpec, String> {
    tool_spec(std::env::consts::OS, std::env::consts::ARCH)
        .ok_or_else(|| "DepotDownloader is not available for this computer".into())
}

fn tool_spec(os: &str, arch: &str) -> Option<ToolSpec> {
    match (os, arch) {
        ("macos", "aarch64") => Some(ToolSpec {
            archive: "DepotDownloader-macos-arm64.zip",
            sha256: "60e80c7c496f3f9a079cd3c62036b35d088c27bc0149baf38f009eb57a52f6a5",
            executable: "DepotDownloader",
            executable_sha256: "e5588dff802b7395b22b308ee796dd57bacbbbb2decddb12fd94101cb4f40fa5",
        }),
        ("macos", "x86_64") => Some(ToolSpec {
            archive: "DepotDownloader-macos-x64.zip",
            sha256: "3214b689564d73e9342a8a4aef693de6ad3d293801b0f300a4466f60ec75befb",
            executable: "DepotDownloader",
            executable_sha256: "8433ea659f93fffb3c0da3400c2fd71393238db35d7780cf5a449fe8b8b10dba",
        }),
        ("linux", "aarch64") => Some(ToolSpec {
            archive: "DepotDownloader-linux-arm64.zip",
            sha256: "d9fb612ccebc1db8eeea3b4045d2221ec70431381393ce908fb72f01d4f9c812",
            executable: "DepotDownloader",
            executable_sha256: "07190213d6eb59799a59fb2cc763f52f51a59bfe8ad0f72c36044fff0fce9a09",
        }),
        ("linux", "x86_64") => Some(ToolSpec {
            archive: "DepotDownloader-linux-x64.zip",
            sha256: "a999dec66b4850fc961bd50366696d23c2d0fad7b18790e6a5647b2f19097a53",
            executable: "DepotDownloader",
            executable_sha256: "d62a1721564bdb96bacd9285bb5f96180a45202e82a9f85c6a88e5e8ee5f992c",
        }),
        ("windows", "x86_64") => Some(ToolSpec {
            archive: "DepotDownloader-windows-x64.zip",
            sha256: "41c9e9f0df54b3ad02e67a11726756e5c73283bd7c2e1b04acfa5ae4c2ed3767",
            executable: "DepotDownloader.exe",
            executable_sha256: "6281279efce8f1e20db9532a58e42382f81afb9e3827a8b965ffcb43fbe4531f",
        }),
        _ => None,
    }
}

fn io_error(context: &'static str) -> impl FnOnce(std::io::Error) -> String {
    move |error| format!("{context}: {error}")
}

#[cfg(test)]
mod tests {
    use super::{
        decode_output_line, output_kind, percent_in_line, progress_for_line, replace_directory,
        strip_ansi, tool_spec,
    };
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn selects_pinned_tools_for_release_targets() {
        for target in [
            ("macos", "aarch64"),
            ("macos", "x86_64"),
            ("linux", "aarch64"),
            ("linux", "x86_64"),
            ("windows", "x86_64"),
        ] {
            let spec = tool_spec(target.0, target.1).unwrap();
            assert_eq!(spec.sha256.len(), 64);
            assert_eq!(spec.executable_sha256.len(), 64);
            assert!(spec.archive.ends_with(".zip"));
        }
        assert!(tool_spec("windows", "aarch64").is_none());
    }

    #[test]
    fn maps_steam_output_to_progress() {
        assert_eq!(progress_for_line("Use the Steam Mobile App"), 16);
        assert_eq!(progress_for_line("Downloading depot 413151"), 30);
        assert_eq!(progress_for_line("Downloading: 50.25%"), 60);
        assert_eq!(progress_for_line("Depot download complete"), 92);
        assert_eq!(percent_in_line("Downloading: 99.90%"), Some(99));
    }

    #[test]
    fn removes_terminal_colours_without_damaging_qr_text() {
        assert_eq!(strip_ansi("\u{1b}[32m██ QR ██\u{1b}[0m"), "██ QR ██");
    }

    #[test]
    fn separates_qr_rows_from_download_logs() {
        assert_eq!(output_kind("Use the Steam Mobile App", false), "qr-start");
        assert_eq!(output_kind("    ██████    ██", false), "qr");
        assert_eq!(output_kind("                    ", false), "qr");
        assert_eq!(output_kind("Downloading depot 413151", false), "log");
        assert_eq!(output_kind("Connection failed", true), "warning");
    }

    #[test]
    fn reads_windows_console_qr_rows_without_dropping_them() {
        let line = decode_output_line(b"    \xdb\xdb  \xdb\xdb  \xdb\xdb  \xdb\xdb\r\n");
        assert_eq!(line, "    ██  ██  ██  ██");
        assert_eq!(output_kind(&line, false), "qr");
    }

    #[test]
    fn replaces_a_cached_download_without_mixing_files() {
        let temp = tempdir().unwrap();
        let current = temp.path().join("game");
        let download = temp.path().join("game.download");
        fs::create_dir_all(&current).unwrap();
        fs::create_dir_all(&download).unwrap();
        fs::write(current.join("old"), b"old").unwrap();
        fs::write(download.join("new"), b"new").unwrap();

        replace_directory(&download, &current).unwrap();

        assert!(!current.join("old").exists());
        assert_eq!(fs::read(current.join("new")).unwrap(), b"new");
        assert!(!download.exists());
        assert!(!temp.path().join("game.previous").exists());
    }
}
