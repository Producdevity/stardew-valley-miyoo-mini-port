use crate::hash;
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

const GAME_EXE: &str = "Stardew Valley.exe";
const EXPECTED_XNB_COUNT: usize = 3550;
const VERSION_1614: &str = "505d343f04420186ba2b611bcc5d256eff554451f55a6b37f3454362d5e03656";
const VERSION_1615: &str = "0cb091faf1c3ade402340641fc47bcf9a8f6e591a645f27a4c0db2fcdc966086";
const XTILE_1614: &str = "a05a1123aa3abb8c68ec2589649dfac724dd3cc52a2e0d812f04ffab794a7be5";
const XTILE_1615: &str = "889b89f06e9699f449b448ac0e9d332c1bee61488f68e590dcb48b16867b293e";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameCandidate {
    pub path: String,
    pub source: String,
    pub supported: bool,
    pub version: Option<&'static str>,
    pub detail: String,
}

pub fn detect_game() -> Option<GameCandidate> {
    for steamapps in steam_library_paths() {
        let manifest = steamapps.join("appmanifest_413150.acf");
        let install_name = fs::read_to_string(&manifest)
            .ok()
            .and_then(|text| vdf_value(&text, "installdir"))
            .unwrap_or_else(|| "Stardew Valley".into());
        let path = steamapps.join("common").join(install_name);
        if path.join(GAME_EXE).is_file() {
            return Some(inspect_game(path, "Steam library"));
        }
    }
    None
}

pub fn inspect_game(path: PathBuf, source: &str) -> GameCandidate {
    let canonical = path.canonicalize().unwrap_or(path);
    let executable = canonical.join(GAME_EXE);
    let xtile = canonical.join("xTile.dll");
    let content = canonical.join("Content");
    let game_hash = hash::sha256_file(&executable).ok();
    let xtile_hash = hash::sha256_file(&xtile).ok();
    let game = game_hash.as_deref().and_then(game_for_hash);
    let version = game.map(|(version, _)| version);
    let xtile_matches = game
        .zip(xtile_hash.as_deref())
        .map(|((_, expected), actual)| actual == expected)
        .unwrap_or(false);
    let xnb_count = content.is_dir().then(|| count_xnb_files(&content));
    let detail = if !executable.is_file() {
        "Stardew Valley.exe is missing".into()
    } else if !xtile.is_file() {
        "xTile.dll is missing".into()
    } else if game.is_none() {
        format!(
            "Unsupported Stardew Valley.exe ({})",
            short_hash(game_hash.as_deref())
        )
    } else if !xtile_matches {
        format!(
            "xTile.dll does not match compatibility build {} ({})",
            version.expect("known game hash has a version"),
            short_hash(xtile_hash.as_deref())
        )
    } else if !content.is_dir() {
        "Content directory is missing".into()
    } else if xnb_count != Some(EXPECTED_XNB_COUNT) {
        format!(
            "Incomplete Content directory: found {} of {} XNB files",
            xnb_count.unwrap_or(0),
            EXPECTED_XNB_COUNT
        )
    } else if let Some(version) = version {
        format!("Compatibility build {version}")
    } else {
        "This Stardew build is not supported".into()
    };

    GameCandidate {
        path: canonical.to_string_lossy().into_owned(),
        source: source.into(),
        supported: version.is_some() && xtile_matches && xnb_count == Some(EXPECTED_XNB_COUNT),
        version,
        detail,
    }
}

pub(crate) fn game_for_hash(game_hash: &str) -> Option<(&'static str, &'static str)> {
    match game_hash {
        VERSION_1614 => Some(("1.6.14.24317", XTILE_1614)),
        VERSION_1615 => Some(("1.6.15.24356", XTILE_1615)),
        _ => None,
    }
}

fn count_xnb_files(root: &std::path::Path) -> usize {
    let Ok(entries) = fs::read_dir(root) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| {
            let Ok(file_type) = entry.file_type() else {
                return 0;
            };
            if file_type.is_dir() {
                count_xnb_files(&entry.path())
            } else if file_type.is_file()
                && entry
                    .path()
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .map(|extension| extension.eq_ignore_ascii_case("xnb"))
                    .unwrap_or(false)
            {
                1
            } else {
                0
            }
        })
        .sum()
}

fn short_hash(hash: Option<&str>) -> &str {
    hash.and_then(|value| value.get(..12))
        .unwrap_or("unreadable")
}

pub fn unsupported(path: PathBuf, source: &str, detail: String) -> GameCandidate {
    GameCandidate {
        path: path.to_string_lossy().into_owned(),
        source: source.into(),
        supported: false,
        version: None,
        detail,
    }
}

fn steam_library_paths() -> Vec<PathBuf> {
    let mut libraries = Vec::new();
    let mut seen = HashSet::new();

    for root in platform_steam_roots() {
        let steamapps = root.join("steamapps");
        add_path(&mut libraries, &mut seen, steamapps.clone());
        let library_file = steamapps.join("libraryfolders.vdf");
        if let Ok(text) = fs::read_to_string(library_file) {
            for path in vdf_library_paths(&text) {
                add_path(&mut libraries, &mut seen, path.join("steamapps"));
            }
        }
    }
    libraries
}

fn add_path(paths: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, path: PathBuf) {
    if seen.insert(path.clone()) {
        paths.push(path);
    }
}

fn platform_steam_roots() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    if cfg!(target_os = "macos") {
        vec![home.join("Library/Application Support/Steam")]
    } else if cfg!(target_os = "windows") {
        let mut roots = Vec::new();
        if let Some(program_files) = std::env::var_os("PROGRAMFILES(X86)") {
            roots.push(PathBuf::from(program_files).join("Steam"));
        }
        roots.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
        roots
    } else {
        vec![
            home.join(".local/share/Steam"),
            home.join(".steam/steam"),
            home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
        ]
    }
}

fn vdf_library_paths(text: &str) -> Vec<PathBuf> {
    let tokens = quoted_tokens(text);
    let mut paths = Vec::new();
    for pair in tokens.windows(2) {
        let named_path = pair[0].eq_ignore_ascii_case("path");
        let legacy_path = pair[0].chars().all(|c| c.is_ascii_digit()) && looks_like_path(&pair[1]);
        if named_path || legacy_path {
            paths.push(PathBuf::from(pair[1].replace("\\\\", "\\")));
        }
    }
    paths
}

fn looks_like_path(value: &str) -> bool {
    value.starts_with('/') || value.contains(":\\") || value.contains(":/")
}

fn vdf_value(text: &str, key: &str) -> Option<String> {
    let tokens = quoted_tokens(text);
    tokens
        .windows(2)
        .find(|pair| pair[0].eq_ignore_ascii_case(key))
        .map(|pair| pair[1].clone())
}

fn quoted_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '"' {
            continue;
        }
        let mut token = String::new();
        while let Some(ch) = chars.next() {
            match ch {
                '"' => break,
                '\\' => {
                    if let Some(next) = chars.next() {
                        token.push(next);
                    }
                }
                _ => token.push(ch),
            }
        }
        tokens.push(token);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::{game_for_hash, quoted_tokens, vdf_library_paths, vdf_value, VERSION_1615};

    #[test]
    fn reads_manifest_values() {
        let input = r#""AppState" { "installdir" "Stardew Valley" }"#;
        assert_eq!(
            vdf_value(input, "installdir").as_deref(),
            Some("Stardew Valley")
        );
    }

    #[test]
    fn reads_modern_library_paths() {
        let input = r#""libraryfolders" { "1" { "path" "/mnt/games" } }"#;
        assert_eq!(vdf_library_paths(input)[0].to_string_lossy(), "/mnt/games");
    }

    #[test]
    fn handles_escaped_quotes() {
        assert_eq!(
            quoted_tokens(r#""a" "some \"quoted\" value""#)[1],
            "some \"quoted\" value"
        );
    }

    #[test]
    fn identifies_the_game_before_checking_companion_files() {
        let (version, xtile) = game_for_hash(VERSION_1615).unwrap();
        assert_eq!(version, "1.6.15.24356");
        assert_eq!(xtile.len(), 64);
    }
}
