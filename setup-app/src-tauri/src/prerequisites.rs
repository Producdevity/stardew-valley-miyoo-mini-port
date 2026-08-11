use crate::platform;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Requirement {
    pub name: &'static str,
    pub ready: bool,
    pub detail: String,
    pub help_url: Option<&'static str>,
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
    match platform::wsl_distribution() {
        Some(distribution) => Requirement {
            name: "WSL",
            ready: true,
            detail: format!("Using {distribution}"),
            help_url: Some("https://learn.microsoft.com/windows/wsl/install"),
        },
        None => Requirement {
            name: "WSL",
            ready: false,
            detail: "Install Ubuntu with: wsl --install -d Ubuntu".into(),
            help_url: Some("https://learn.microsoft.com/windows/wsl/install"),
        },
    }
}

fn inspect_mono() -> Requirement {
    match output("mono", &["--version"]) {
        Some((true, text)) => Requirement {
            name: "Mono 6",
            ready: text.contains("version 6."),
            detail: first_line(&text),
            help_url: Some("https://www.mono-project.com/download/stable/"),
        },
        _ if cfg!(target_os = "windows") => missing(
            "Mono 6",
            "Open the Linux distribution shown above, then run: sudo apt update && sudo apt install mono-devel",
            "https://www.mono-project.com/download/stable/",
        ),
        _ => missing(
            "Mono 6",
            "Mono was not found",
            "https://www.mono-project.com/download/stable/",
        ),
    }
}

fn inspect_sgen() -> Requirement {
    match output("sgen", &["--help"]) {
        Some((true, _)) => Requirement {
            name: "Mono serializer",
            ready: true,
            detail: "sgen is available".into(),
            help_url: Some("https://www.mono-project.com/download/stable/"),
        },
        _ if cfg!(target_os = "windows") => missing(
            "Mono serializer",
            "Open the Linux distribution shown above, then run: sudo apt update && sudo apt install mono-devel",
            "https://www.mono-project.com/download/stable/",
        ),
        _ => missing(
            "Mono serializer",
            "sgen was not found",
            "https://www.mono-project.com/download/stable/",
        ),
    }
}

fn inspect_docker() -> Requirement {
    match output("docker", &["version", "--format", "{{.Server.Version}}"]) {
        Some((true, text)) if !text.trim().is_empty() => Requirement {
            name: "Docker",
            ready: true,
            detail: format!("Docker {}", text.trim()),
            help_url: Some("https://docs.docker.com/get-docker/"),
        },
        _ if cfg!(target_os = "windows") && platform::windows_docker_running() => missing(
            "Docker",
            "Docker Desktop is running; enable WSL integration for the Linux distribution shown above",
            "https://docs.docker.com/desktop/features/wsl/",
        ),
        Some((false, text)) if docker_permission_denied(&text) => missing(
            "Docker",
            "Docker is installed, but this user cannot access it",
            "https://docs.docker.com/engine/install/linux-postinstall/",
        ),
        Some(_) if cfg!(target_os = "linux") => missing(
            "Docker",
            "Docker is installed but the service is not available",
            "https://docs.docker.com/engine/install/linux-postinstall/",
        ),
        Some(_) => missing(
            "Docker",
            "Docker is installed but not running",
            "https://docs.docker.com/get-docker/",
        ),
        None => missing(
            "Docker",
            "Docker was not found or is not running",
            "https://docs.docker.com/get-docker/",
        ),
    }
}

fn docker_permission_denied(text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    text.contains("permission denied") || text.contains("access is denied")
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

fn missing(name: &'static str, detail: &str, help_url: &'static str) -> Requirement {
    Requirement {
        name,
        ready: false,
        detail: detail.into(),
        help_url: Some(help_url),
    }
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or(text).trim().to_owned()
}

#[cfg(test)]
mod tests {
    #[test]
    fn recognizes_docker_socket_permission_errors() {
        assert!(super::docker_permission_denied(
            "permission denied while trying to connect to the Docker daemon socket"
        ));
        assert!(!super::docker_permission_denied(
            "Cannot connect to the Docker daemon"
        ));
    }
}
