use crate::hash;
use crate::steam::{self, GameCandidate};
use crate::AppState;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use tauri::{AppHandle, Manager};
use zip::ZipArchive;

const GAME_EXE: &str = "Stardew Valley.exe";
const EXPECTED_XNB_COUNT: usize = 3550;
const MAX_ARCHIVE_ENTRIES: usize = 20_000;
const MAX_UNCOMPRESSED_SIZE: u64 = 2 * 1024 * 1024 * 1024;

pub fn inspect(path: PathBuf) -> GameCandidate {
    if path.is_dir() {
        return steam::inspect_game(path, "Selected folder");
    }
    if is_zip(&path) {
        return inspect_zip(&path)
            .unwrap_or_else(|error| steam::unsupported(path, "ZIP archive", error));
    }
    steam::unsupported(
        path,
        "Selected file",
        "Choose the game folder or a ZIP archive".into(),
    )
}

pub fn resolve(app: &AppHandle, state: &AppState, path: PathBuf) -> Result<PathBuf, String> {
    if path.is_dir() {
        return path
            .canonicalize()
            .map_err(|error| format!("Could not open the game folder: {error}"));
    }
    if !is_zip(&path) {
        return Err("Choose the game folder or a ZIP archive".into());
    }
    let _guard = state
        .source_lock
        .lock()
        .map_err(|_| "Game archive cache lock failed".to_string())?;
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("Could not open the app cache: {error}"))?
        .join("game-sources");
    resolve_zip(&path, &cache)
}

fn inspect_zip(path: &Path) -> Result<GameCandidate, String> {
    let file =
        fs::File::open(path).map_err(|error| format!("Could not open ZIP archive: {error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("Invalid ZIP archive: {error}"))?;
    let layout = archive_layout(&mut archive)?;
    let game_hash = {
        let executable = archive
            .by_index(layout.exe_index)
            .map_err(|error| format!("Could not read {GAME_EXE}: {error}"))?;
        hash::sha256_reader(executable)
            .map_err(|error| format!("Could not verify {GAME_EXE}: {error}"))?
    };
    let xtile_hash = {
        let xtile = archive
            .by_index(layout.xtile_index)
            .map_err(|error| format!("Could not read xTile.dll: {error}"))?;
        hash::sha256_reader(xtile)
            .map_err(|error| format!("Could not verify xTile.dll: {error}"))?
    };
    let version = steam::version_for_hashes(&game_hash, &xtile_hash);
    let supported = version.is_some() && layout.xnb_count == EXPECTED_XNB_COUNT;
    let detail = if let Some(version) = version {
        if layout.xnb_count == EXPECTED_XNB_COUNT {
            format!("Compatibility build {version} ZIP")
        } else {
            format!(
                "Incomplete Content directory: found {} of {} files",
                layout.xnb_count, EXPECTED_XNB_COUNT
            )
        }
    } else {
        "This Stardew build is not supported".into()
    };
    Ok(GameCandidate {
        path: path.to_string_lossy().into_owned(),
        source: "ZIP archive".into(),
        supported,
        version,
        detail,
    })
}

fn resolve_zip(source: &Path, cache: &Path) -> Result<PathBuf, String> {
    let source_hash = hash::sha256_file(source)
        .map_err(|error| format!("Could not verify the ZIP archive: {error}"))?;
    let root = cache.join(&source_hash);
    let game = root.join("game");
    let marker = root.join("source.sha256");
    if fs::read_to_string(&marker).ok().as_deref() == Some(source_hash.as_str())
        && game.join(GAME_EXE).is_file()
    {
        return Ok(game);
    }

    fs::create_dir_all(cache).map_err(io_error("Could not create the game archive cache"))?;
    let staging = cache.join(format!("{source_hash}.tmp"));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(io_error("Could not clear the game archive cache"))?;
    }
    fs::create_dir_all(staging.join("game"))
        .map_err(io_error("Could not create the game archive cache"))?;
    extract_zip(source, &staging.join("game"))?;
    fs::write(staging.join("source.sha256"), &source_hash)
        .map_err(io_error("Could not finalize the game archive cache"))?;
    if root.exists() {
        fs::remove_dir_all(&root).map_err(io_error("Could not replace the game archive cache"))?;
    }
    fs::rename(&staging, &root).map_err(io_error("Could not finalize the game archive cache"))?;
    Ok(game)
}

fn extract_zip(source: &Path, destination: &Path) -> Result<(), String> {
    let file = fs::File::open(source).map_err(io_error("Could not open the ZIP archive"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("Invalid ZIP archive: {error}"))?;
    let layout = archive_layout(&mut archive)?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Could not read the ZIP archive: {error}"))?;
        if entry.is_symlink() {
            return Err(format!(
                "ZIP archive contains a symbolic link: {}",
                entry.name()
            ));
        }
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| format!("ZIP archive contains an unsafe path: {}", entry.name()))?;
        let relative = enclosed.strip_prefix(&layout.root).map_err(|_| {
            format!(
                "ZIP archive contains files outside the game folder: {}",
                entry.name()
            )
        })?;
        if relative.as_os_str().is_empty() || ignored(relative) {
            continue;
        }
        let target = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(io_error("Could not create a game directory"))?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(io_error("Could not create a game directory"))?;
        }
        let mut output =
            fs::File::create(&target).map_err(io_error("Could not extract a game file"))?;
        io::copy(&mut entry, &mut output).map_err(io_error("Could not extract a game file"))?;
    }
    Ok(())
}

struct ArchiveLayout {
    root: PathBuf,
    exe_index: usize,
    xtile_index: usize,
    xnb_count: usize,
}

fn archive_layout(archive: &mut ZipArchive<fs::File>) -> Result<ArchiveLayout, String> {
    let mut uncompressed_size = 0_u64;
    let mut executable = None;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("Could not read the ZIP archive: {error}"))?;
        uncompressed_size = uncompressed_size
            .checked_add(entry.size())
            .ok_or_else(|| "ZIP archive is too large".to_string())?;
        let path = entry
            .enclosed_name()
            .ok_or_else(|| format!("ZIP archive contains an unsafe path: {}", entry.name()))?;
        if path.file_name().and_then(|name| name.to_str()) == Some(GAME_EXE) {
            if executable.is_some() {
                return Err(format!("ZIP archive contains more than one {GAME_EXE}"));
            }
            executable = Some((index, path.parent().unwrap_or(Path::new("")).to_owned()));
        }
    }
    validate_archive_limits(archive.len(), uncompressed_size)?;
    let (exe_index, root) =
        executable.ok_or_else(|| format!("ZIP archive does not contain {GAME_EXE}"))?;
    if root.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err("ZIP archive has an unsafe game directory".into());
    }

    let content = root.join("Content");
    let xtile_path = root.join("xTile.dll");
    let mut xtile_index = None;
    let mut xnb_count = 0;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("Could not read the ZIP archive: {error}"))?;
        let Some(path) = entry.enclosed_name() else {
            continue;
        };
        if path == xtile_path {
            xtile_index = Some(index);
        }
        if path.starts_with(&content)
            && path.extension().and_then(|extension| extension.to_str()) == Some("xnb")
        {
            xnb_count += 1;
        }
    }
    Ok(ArchiveLayout {
        root,
        exe_index,
        xtile_index: xtile_index
            .ok_or_else(|| "ZIP archive does not contain xTile.dll".to_string())?,
        xnb_count,
    })
}

fn validate_archive_limits(entries: usize, uncompressed_size: u64) -> Result<(), String> {
    if entries > MAX_ARCHIVE_ENTRIES {
        return Err("ZIP archive contains too many files".into());
    }
    if uncompressed_size > MAX_UNCOMPRESSED_SIZE {
        return Err("ZIP archive is too large".into());
    }
    Ok(())
}

fn ignored(path: &Path) -> bool {
    matches!(
        path.components().next(),
        Some(Component::Normal(name)) if name == ".DepotDownloader" || name == "__MACOSX"
    ) || matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".DS_Store") | Some("Thumbs.db")
    )
}

fn is_zip(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
}

fn io_error(context: &'static str) -> impl FnOnce(io::Error) -> String {
    move |error| format!("{context}: {error}")
}

#[cfg(test)]
mod tests {
    use super::{
        extract_zip, inspect, resolve_zip, validate_archive_limits, MAX_ARCHIVE_ENTRIES,
        MAX_UNCOMPRESSED_SIZE,
    };
    use std::fs;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;

    fn create_zip(path: &std::path::Path, entries: &[(&str, &[u8])]) {
        let file = fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        for (name, data) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn rejects_traversal_paths() {
        let temp = tempdir().unwrap();
        let archive = temp.path().join("game.zip");
        create_zip(
            &archive,
            &[
                ("Game/Stardew Valley.exe", b"test"),
                ("Game/../../outside", b"bad"),
            ],
        );
        let error = extract_zip(&archive, &temp.path().join("out")).unwrap_err();
        assert!(error.contains("unsafe path"));
    }

    #[test]
    fn skips_depot_downloader_metadata() {
        let temp = tempdir().unwrap();
        let archive = temp.path().join("game.zip");
        create_zip(
            &archive,
            &[
                ("Game/Stardew Valley.exe", b"test"),
                ("Game/xTile.dll", b"test"),
                ("Game/Content/file.xnb", b"content"),
                ("Game/.DepotDownloader/depot.config", b"metadata"),
            ],
        );
        let output = resolve_zip(&archive, &temp.path().join("cache")).unwrap();
        assert!(output.join("Content/file.xnb").is_file());
        assert!(!output.join(".DepotDownloader").exists());
    }

    #[test]
    fn reports_unsupported_archives_without_panicking() {
        let temp = tempdir().unwrap();
        let archive = temp.path().join("game.zip");
        create_zip(&archive, &[("Game/readme.txt", b"not a game")]);
        let candidate = inspect(archive);
        assert!(!candidate.supported);
        assert!(candidate.detail.contains("does not contain"));
    }

    #[test]
    fn rejects_archive_limits() {
        assert!(validate_archive_limits(MAX_ARCHIVE_ENTRIES + 1, 0).is_err());
        assert!(validate_archive_limits(1, MAX_UNCOMPRESSED_SIZE + 1).is_err());
    }

    #[test]
    #[ignore = "requires SVMM_TEST_GAME_ZIP"]
    fn accepts_an_external_supported_game_archive() {
        let path = std::env::var_os("SVMM_TEST_GAME_ZIP")
            .map(std::path::PathBuf::from)
            .expect("SVMM_TEST_GAME_ZIP is not set");
        let candidate = inspect(path);
        assert!(candidate.supported, "{}", candidate.detail);
        assert_eq!(candidate.version, Some("1.6.15.24356"));
    }

    #[test]
    #[ignore = "requires SVMM_TEST_GAME_ZIP and SVMM_TEST_RELEASE_ROOT"]
    fn extracts_an_external_archive_for_release_validation() {
        let source = std::env::var_os("SVMM_TEST_GAME_ZIP")
            .map(std::path::PathBuf::from)
            .expect("SVMM_TEST_GAME_ZIP is not set");
        let release = std::env::var_os("SVMM_TEST_RELEASE_ROOT")
            .map(std::path::PathBuf::from)
            .expect("SVMM_TEST_RELEASE_ROOT is not set");
        let temp = tempdir().unwrap();
        let game = resolve_zip(&source, &temp.path().join("cache")).unwrap();
        assert!(!game.join(".DepotDownloader").exists());
        let status = std::process::Command::new("/bin/sh")
            .arg(release.join("scripts/check-gamefiles.sh"))
            .arg(game)
            .status()
            .unwrap();
        assert!(status.success());
    }
}
