use crate::platform;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Requirement {
    pub name: &'static str,
    pub ready: bool,
    pub detail: String,
}

pub fn inspect() -> Vec<Requirement> {
    let mut requirements = Vec::new();
    if cfg!(target_os = "windows") {
        requirements.push(inspect_wsl());
    }
    requirements.extend([inspect_mono(), inspect_sgen(), inspect_docker()]);
    requirements
}

fn inspect_wsl() -> Requirement {
    let ready = platform::wsl_ready();
    Requirement {
        name: "WSL",
        ready,
        detail: if ready {
            "Windows Subsystem for Linux is available".into()
        } else {
            "Install WSL and its default Linux distribution".into()
        },
    }
}

fn inspect_mono() -> Requirement {
    match output("mono", &["--version"]) {
        Some((_, text)) => Requirement {
            name: "Mono 6",
            ready: text.contains("version 6."),
            detail: first_line(&text),
        },
        None => missing("Mono 6", "Mono was not found"),
    }
}

fn inspect_sgen() -> Requirement {
    match output("sgen", &["--help"]) {
        Some(_) => Requirement {
            name: "Mono serializer",
            ready: true,
            detail: "sgen is available".into(),
        },
        None => missing("Mono serializer", "sgen was not found"),
    }
}

fn inspect_docker() -> Requirement {
    match output("docker", &["version", "--format", "{{.Server.Version}}"]) {
        Some((true, text)) if !text.trim().is_empty() => Requirement {
            name: "Docker",
            ready: true,
            detail: format!("Docker {}", text.trim()),
        },
        Some(_) => missing("Docker", "Docker is installed but not running"),
        None => missing("Docker", "Docker was not found or is not running"),
    }
}

fn output(command: &str, args: &[&str]) -> Option<(bool, String)> {
    let result = platform::tool_output(command, args)?;
    let mut text = String::from_utf8_lossy(&result.stdout).into_owned();
    if text.trim().is_empty() {
        text = String::from_utf8_lossy(&result.stderr).into_owned();
    }
    if result.status.success() || !text.trim().is_empty() {
        Some((result.status.success(), text))
    } else {
        None
    }
}

fn missing(name: &'static str, detail: &str) -> Requirement {
    Requirement {
        name,
        ready: false,
        detail: detail.into(),
    }
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or(text).trim().to_owned()
}
