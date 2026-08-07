use std::ffi::OsString;
use std::path::Path;
#[cfg(not(target_os = "windows"))]
use std::path::PathBuf;
use std::process::{Command, Output};

pub fn tool_output(program: &str, args: &[&str]) -> Option<Output> {
    tool_command(program, args).output().ok()
}

pub fn wsl_ready() -> bool {
    #[cfg(target_os = "windows")]
    {
        Command::new("wsl.exe")
            .args(["--exec", "/bin/true"])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "windows"))]
    {
        true
    }
}

pub fn windows_docker_running() -> bool {
    #[cfg(target_os = "windows")]
    {
        let mut candidates = vec![std::path::PathBuf::from("docker.exe")];
        if let Some(program_files) = std::env::var_os("PROGRAMFILES") {
            candidates.push(
                std::path::PathBuf::from(program_files)
                    .join("Docker/Docker/resources/bin/docker.exe"),
            );
        }
        candidates.into_iter().any(|program| {
            Command::new(program)
                .args(["version", "--format", "{{.Server.Version}}"])
                .output()
                .map(|output| output.status.success() && !output.stdout.is_empty())
                .unwrap_or(false)
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

pub fn script_command(script: &Path, paths: &[&Path]) -> Result<Command, String> {
    #[cfg(target_os = "windows")]
    {
        let script = to_wsl_path(script)?;
        let translated = paths
            .iter()
            .map(|path| to_wsl_path(path))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(command_from_spec(windows_script_spec(&script, &translated)))
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(command_from_spec(unix_script_spec(script, paths)))
    }
}

pub fn terminate_process_tree(pid: u32, force: bool) {
    #[cfg(target_os = "windows")]
    {
        let _ = force;
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status();
    }
    #[cfg(unix)]
    unsafe {
        // The child starts in its own process group, so a negative PID stops
        // the shell and every compiler or container process it launched.
        libc::kill(
            -(pid as i32),
            if force { libc::SIGKILL } else { libc::SIGTERM },
        );
    }
}

pub fn configure_process_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(target_os = "windows")]
    let _ = command;
}

fn tool_command(program: &str, args: &[&str]) -> Command {
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("wsl.exe");
        command.args(["--exec", program]).args(args);
        command
    }
    #[cfg(not(target_os = "windows"))]
    {
        let executable = resolve_tool(program).unwrap_or_else(|| PathBuf::from(program));
        let mut command = Command::new(executable);
        command.env("PATH", execution_path());
        command.args(args);
        command
    }
}

#[cfg(target_os = "windows")]
fn to_wsl_path(path: &Path) -> Result<String, String> {
    let output = Command::new("wsl.exe")
        .args(["--exec", "wslpath", "-a", "-u"])
        .arg(path)
        .output()
        .map_err(|error| format!("Could not translate a Windows path for WSL: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if detail.is_empty() {
            format!("WSL cannot access {}", path.display())
        } else {
            format!("WSL cannot access {}: {detail}", path.display())
        });
    }
    let translated = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if translated.is_empty() {
        Err(format!("WSL cannot access {}", path.display()))
    } else {
        Ok(translated)
    }
}

#[derive(Debug, PartialEq)]
struct CommandSpec {
    program: OsString,
    args: Vec<OsString>,
}

#[cfg(any(not(target_os = "windows"), test))]
fn unix_script_spec(script: &Path, paths: &[&Path]) -> CommandSpec {
    let mut args = vec![script.as_os_str().to_owned()];
    args.extend(paths.iter().map(|path| path.as_os_str().to_owned()));
    CommandSpec {
        program: OsString::from("/bin/sh"),
        args,
    }
}

#[cfg(any(target_os = "windows", test))]
fn windows_script_spec(script: &str, paths: &[String]) -> CommandSpec {
    let mut args = vec![
        OsString::from("--exec"),
        OsString::from("/bin/sh"),
        OsString::from(script),
    ];
    args.extend(paths.iter().map(OsString::from));
    CommandSpec {
        program: OsString::from("wsl.exe"),
        args,
    }
}

fn command_from_spec(spec: CommandSpec) -> Command {
    let mut command = Command::new(spec.program);
    #[cfg(not(target_os = "windows"))]
    command.env("PATH", execution_path());
    command.args(spec.args);
    command
}

#[cfg(not(target_os = "windows"))]
fn resolve_tool(program: &str) -> Option<PathBuf> {
    std::env::split_paths(&execution_path())
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
}

#[cfg(not(target_os = "windows"))]
fn execution_path() -> OsString {
    let mut paths = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();

    #[cfg(target_os = "macos")]
    {
        push_path(
            &mut paths,
            PathBuf::from("/Library/Frameworks/Mono.framework/Versions/Current/Commands"),
        );
        push_path(
            &mut paths,
            PathBuf::from("/Applications/Docker.app/Contents/Resources/bin"),
        );
        if let Some(home) = dirs::home_dir() {
            push_path(&mut paths, home.join(".docker/bin"));
        }
        push_path(&mut paths, PathBuf::from("/opt/homebrew/bin"));
    }

    for path in [
        "/usr/local/bin",
        "/usr/local/sbin",
        "/usr/bin",
        "/usr/sbin",
        "/bin",
        "/sbin",
    ] {
        push_path(&mut paths, PathBuf::from(path));
    }

    std::env::join_paths(paths).unwrap_or_else(|_| OsString::from("/usr/local/bin:/usr/bin:/bin"))
}

#[cfg(not(target_os = "windows"))]
fn push_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

#[cfg(test)]
mod tests {
    use super::{unix_script_spec, windows_script_spec, CommandSpec};
    use std::ffi::OsString;
    use std::path::Path;

    #[test]
    fn unix_scripts_receive_paths_as_separate_arguments() {
        let spec = unix_script_spec(
            Path::new("/app/prepare.sh"),
            &[Path::new("/games/Stardew Valley"), Path::new("/tmp/output")],
        );
        assert_eq!(
            spec,
            CommandSpec {
                program: OsString::from("/bin/sh"),
                args: vec![
                    OsString::from("/app/prepare.sh"),
                    OsString::from("/games/Stardew Valley"),
                    OsString::from("/tmp/output"),
                ],
            }
        );
    }

    #[test]
    fn windows_scripts_use_wsl_without_shell_interpolation() {
        let spec = windows_script_spec(
            "/mnt/c/app/prepare.sh",
            &["/mnt/d/Games/Stardew Valley".into(), "/mnt/c/out".into()],
        );
        assert_eq!(spec.program, OsString::from("wsl.exe"));
        assert_eq!(
            spec.args,
            vec![
                "--exec",
                "/bin/sh",
                "/mnt/c/app/prepare.sh",
                "/mnt/d/Games/Stardew Valley",
                "/mnt/c/out",
            ]
            .into_iter()
            .map(OsString::from)
            .collect::<Vec<_>>()
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn unix_execution_path_includes_system_tools() {
        let paths = std::env::split_paths(&super::execution_path()).collect::<Vec<_>>();
        assert!(paths.contains(&std::path::PathBuf::from("/usr/bin")));
        assert!(paths.contains(&std::path::PathBuf::from("/bin")));
    }
}
