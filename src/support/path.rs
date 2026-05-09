use crate::error::{ResipError, ResipResult};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn expand_tilde(path: &str) -> ResipResult<PathBuf> {
    if path == "~" {
        return home_dir();
    }

    if let Some(rest) = path.strip_prefix("~/") {
        return Ok(home_dir()?.join(rest));
    }

    if let Some(rest) = path.strip_prefix("~\\") {
        return Ok(home_dir()?.join(rest));
    }

    Ok(PathBuf::from(path))
}

pub fn collapse_home(path: &str) -> ResipResult<String> {
    let expanded = PathBuf::from(path);
    let home = home_dir()?;

    if expanded.is_absolute()
        && let Ok(rest) = expanded.strip_prefix(&home)
    {
        if rest.as_os_str().is_empty() {
            return Ok("~".to_string());
        }
        return Ok(format!("~/{}", rest.to_string_lossy().replace('\\', "/")));
    }

    Ok(path.to_string())
}

pub fn detect_default_identity_file() -> String {
    let candidates = [
        "~/.ssh/id_ed25519",
        "~/.ssh/id_rsa",
        "~/.ssh/id_ecdsa",
        "~/.ssh/id_dsa",
    ];

    for candidate in candidates {
        let exists = match expand_tilde(candidate) {
            Ok(path) => path.is_file(),
            Err(_) => false,
        };
        if exists {
            return candidate.to_string();
        }
    }

    "~/.ssh/id_ed25519".to_string()
}

pub fn default_clash_output_path(name: &str) -> ResipResult<String> {
    let base_dir = match directories::UserDirs::new()
        .and_then(|dirs| dirs.download_dir().map(std::path::Path::to_path_buf))
    {
        Some(path) => path,
        None => home_dir()?,
    };
    let file_name = format!("resip-{}.yaml", sanitize_file_stem(name));
    collapse_home(&base_dir.join(file_name).to_string_lossy())
}

pub fn open_path_dir(path: &Path) -> ResipResult<PathBuf> {
    let dir = match path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        Some(parent) => parent.to_path_buf(),
        None => env::current_dir().map_err(ResipError::CurrentDir)?,
    };

    std::fs::create_dir_all(&dir).map_err(|source| ResipError::CreateDirectory {
        path: dir.display().to_string(),
        source,
    })?;

    let opened_dir = absolute_dir(&dir)?;

    if cfg!(windows) {
        let mut command = Command::new("explorer");
        command.arg(&opened_dir);
        command
            .spawn()
            .map_err(|source| ResipError::OpenDirectory {
                path: opened_dir.display().to_string(),
                source,
            })?;
        return Ok(opened_dir);
    }

    let program = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let status = Command::new(program)
        .arg(&opened_dir)
        .status()
        .map_err(|source| ResipError::OpenDirectory {
            path: opened_dir.display().to_string(),
            source,
        })?;

    if !status.success() {
        return Err(ResipError::FileManagerFailed {
            path: opened_dir.display().to_string(),
        });
    }

    Ok(opened_dir)
}

pub fn home_dir() -> ResipResult<PathBuf> {
    directories::UserDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .ok_or(ResipError::HomeDirUnavailable)
}

fn absolute_dir(dir: &Path) -> ResipResult<PathBuf> {
    if dir.is_absolute() {
        return Ok(dir.to_path_buf());
    }

    Ok(env::current_dir()
        .map_err(ResipError::CurrentDir)?
        .join(dir))
}

fn sanitize_file_stem(value: &str) -> String {
    // Keep generated file names simple and portable across common file systems.
    let mut sanitized = String::new();

    for character in value.to_lowercase().chars() {
        if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
            sanitized.push(character);
        } else if character.is_whitespace() {
            sanitized.push('-');
        }
    }

    let trimmed = sanitized.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "server".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::{expand_tilde, sanitize_file_stem};

    #[test]
    fn sanitize_file_stem_keeps_safe_ascii() {
        assert_eq!(sanitize_file_stem("Resip Server_01"), "resip-server_01");
    }

    #[test]
    fn sanitize_file_stem_falls_back_for_empty_result() {
        assert_eq!(sanitize_file_stem("中文 🚀"), "server");
    }

    #[test]
    fn expand_tilde_leaves_non_tilde_paths_unchanged() {
        assert_eq!(
            expand_tilde("relative/file").unwrap(),
            std::path::PathBuf::from("relative/file")
        );
    }
}
