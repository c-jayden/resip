use crate::error::{ResipError, ResipResult};
use crate::path;
use std::fs;
use std::path::{Path, PathBuf};

const LABEL: &str = "com.resip.tunnel";
#[cfg(windows)]
const WINDOWS_VALUE_NAME: &str = "resip";
#[cfg(windows)]
const WINDOWS_RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(target_os = "linux")]
const LINUX_SERVICE_NAME: &str = "resip.service";

pub struct AutostartInfo {
    pub enabled: bool,
    pub target: String,
}

pub fn enable() -> ResipResult<AutostartInfo> {
    let executable = std::env::current_exe().map_err(ResipError::CurrentExe)?;
    enable_with_executable(&executable)
}

pub fn disable() -> ResipResult<AutostartInfo> {
    disable_current_platform()
}

pub fn status() -> ResipResult<AutostartInfo> {
    status_current_platform()
}

#[cfg(target_os = "macos")]
fn enable_with_executable(executable: &Path) -> ResipResult<AutostartInfo> {
    let path = macos_plist_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ResipError::CreateDirectory {
            path: parent.display().to_string(),
            source,
        })?;
    }
    fs::write(&path, macos_plist_contents(executable)).map_err(|source| ResipError::WriteFile {
        path: path.display().to_string(),
        source,
    })?;
    Ok(AutostartInfo {
        enabled: true,
        target: path.display().to_string(),
    })
}

#[cfg(windows)]
fn enable_with_executable(executable: &Path) -> ResipResult<AutostartInfo> {
    let value = windows_run_value(executable);
    let status = std::process::Command::new("reg")
        .args([
            "add",
            WINDOWS_RUN_KEY,
            "/v",
            WINDOWS_VALUE_NAME,
            "/t",
            "REG_SZ",
            "/d",
            &value,
            "/f",
        ])
        .status()
        .map_err(|source| ResipError::RunCommand {
            program: "reg",
            source,
        })?;
    if !status.success() {
        return Err(ResipError::CommandFailed { program: "reg" });
    }
    Ok(AutostartInfo {
        enabled: true,
        target: format!("{WINDOWS_RUN_KEY}\\{WINDOWS_VALUE_NAME}"),
    })
}

#[cfg(target_os = "linux")]
fn enable_with_executable(executable: &Path) -> ResipResult<AutostartInfo> {
    let path = linux_service_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ResipError::CreateDirectory {
            path: parent.display().to_string(),
            source,
        })?;
    }
    fs::write(&path, linux_service_contents(executable)).map_err(|source| {
        ResipError::WriteFile {
            path: path.display().to_string(),
            source,
        }
    })?;
    run_systemctl_user(&["daemon-reload"])?;
    run_systemctl_user(&["enable", LINUX_SERVICE_NAME])?;
    Ok(AutostartInfo {
        enabled: true,
        target: path.display().to_string(),
    })
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
fn enable_with_executable(_executable: &Path) -> ResipResult<AutostartInfo> {
    Err(ResipError::UnsupportedAutostartPlatform)
}

#[cfg(target_os = "macos")]
fn disable_current_platform() -> ResipResult<AutostartInfo> {
    let path = macos_plist_path()?;
    if path.exists() {
        fs::remove_file(&path).map_err(|source| ResipError::RemoveFile {
            path: path.display().to_string(),
            source,
        })?;
    }
    Ok(AutostartInfo {
        enabled: false,
        target: path.display().to_string(),
    })
}

#[cfg(windows)]
fn disable_current_platform() -> ResipResult<AutostartInfo> {
    let status = std::process::Command::new("reg")
        .args(["delete", WINDOWS_RUN_KEY, "/v", WINDOWS_VALUE_NAME, "/f"])
        .status()
        .map_err(|source| ResipError::RunCommand {
            program: "reg",
            source,
        })?;
    if !status.success() {
        return Err(ResipError::CommandFailed { program: "reg" });
    }
    Ok(AutostartInfo {
        enabled: false,
        target: format!("{WINDOWS_RUN_KEY}\\{WINDOWS_VALUE_NAME}"),
    })
}

#[cfg(target_os = "linux")]
fn disable_current_platform() -> ResipResult<AutostartInfo> {
    let path = linux_service_path()?;
    let _ = run_systemctl_user(&["disable", LINUX_SERVICE_NAME]);
    if path.exists() {
        fs::remove_file(&path).map_err(|source| ResipError::RemoveFile {
            path: path.display().to_string(),
            source,
        })?;
    }
    let _ = run_systemctl_user(&["daemon-reload"]);
    Ok(AutostartInfo {
        enabled: false,
        target: path.display().to_string(),
    })
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
fn disable_current_platform() -> ResipResult<AutostartInfo> {
    Err(ResipError::UnsupportedAutostartPlatform)
}

#[cfg(target_os = "macos")]
fn status_current_platform() -> ResipResult<AutostartInfo> {
    let path = macos_plist_path()?;
    Ok(AutostartInfo {
        enabled: path.exists(),
        target: path.display().to_string(),
    })
}

#[cfg(windows)]
fn status_current_platform() -> ResipResult<AutostartInfo> {
    let status = std::process::Command::new("reg")
        .args(["query", WINDOWS_RUN_KEY, "/v", WINDOWS_VALUE_NAME])
        .status()
        .map_err(|source| ResipError::RunCommand {
            program: "reg",
            source,
        })?;
    Ok(AutostartInfo {
        enabled: status.success(),
        target: format!("{WINDOWS_RUN_KEY}\\{WINDOWS_VALUE_NAME}"),
    })
}

#[cfg(target_os = "linux")]
fn status_current_platform() -> ResipResult<AutostartInfo> {
    let path = linux_service_path()?;
    Ok(AutostartInfo {
        enabled: path.exists(),
        target: path.display().to_string(),
    })
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
fn status_current_platform() -> ResipResult<AutostartInfo> {
    Err(ResipError::UnsupportedAutostartPlatform)
}

fn macos_plist_path() -> ResipResult<PathBuf> {
    Ok(path::home_dir()?
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{LABEL}.plist")))
}

fn macos_plist_contents(executable: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
    <string>on</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
</dict>
</plist>
"#,
        xml_escape(&executable.display().to_string())
    )
}

#[cfg(any(windows, test))]
fn windows_run_value(executable: &Path) -> String {
    format!("\"{}\" on", executable.display())
}

#[cfg(target_os = "linux")]
fn linux_service_path() -> ResipResult<PathBuf> {
    Ok(path::home_dir()?
        .join(".config")
        .join("systemd")
        .join("user")
        .join(LINUX_SERVICE_NAME))
}

#[cfg(any(target_os = "linux", test))]
fn linux_service_contents(executable: &Path) -> String {
    format!(
        r#"[Unit]
Description=resip SSH tunnel

[Service]
Type=oneshot
ExecStart={} on

[Install]
WantedBy=default.target
"#,
        systemd_escape_path(executable)
    )
}

#[cfg(target_os = "linux")]
fn run_systemctl_user(args: &[&str]) -> ResipResult<()> {
    let status = std::process::Command::new("systemctl")
        .arg("--user")
        .args(args)
        .status()
        .map_err(|source| ResipError::RunCommand {
            program: "systemctl",
            source,
        })?;
    if !status.success() {
        return Err(ResipError::CommandFailed {
            program: "systemctl",
        });
    }
    Ok(())
}

#[cfg(any(target_os = "linux", test))]
fn systemd_escape_path(path: &Path) -> String {
    let value = path.display().to_string();
    if value.contains(' ') {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::{linux_service_contents, macos_plist_contents, windows_run_value};
    use std::path::Path;

    #[test]
    fn macos_plist_runs_resip_on_at_login() {
        let contents = macos_plist_contents(Path::new("/Applications/resip"));

        assert!(contents.contains("<string>com.resip.tunnel</string>"));
        assert!(contents.contains("<string>/Applications/resip</string>"));
        assert!(contents.contains("<string>on</string>"));
        assert!(contents.contains("<key>RunAtLoad</key>"));
    }

    #[test]
    fn windows_run_value_starts_resip_on_for_current_user() {
        let value = windows_run_value(Path::new(r"C:\Tools\resip.exe"));

        assert_eq!(value, r#""C:\Tools\resip.exe" on"#);
    }

    #[test]
    fn linux_service_starts_resip_on_as_user_service() {
        let contents = linux_service_contents(Path::new("/usr/local/bin/resip"));

        assert!(contents.contains("Description=resip SSH tunnel"));
        assert!(contents.contains("Type=oneshot"));
        assert!(contents.contains("ExecStart=/usr/local/bin/resip on"));
        assert!(contents.contains("WantedBy=default.target"));
    }
}
