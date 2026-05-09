use crate::error::{ResipError, ResipResult};
use std::env;
use std::io::{self, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn prompt_required(label: &str) -> ResipResult<String> {
    loop {
        print!("{label}: ");
        io::stdout().flush().map_err(ResipError::FlushStdout)?;
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(ResipError::ReadStdin)?;
        let value = input.trim();
        if !value.is_empty() {
            return Ok(value.to_string());
        }
        eprintln!("{label} is required.");
    }
}

pub fn prompt_default(label: &str, default: &str) -> ResipResult<String> {
    print!("{label} [{default}]: ");
    io::stdout().flush().map_err(ResipError::FlushStdout)?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(ResipError::ReadStdin)?;
    let value = input.trim();
    if value.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(value.to_string())
    }
}

pub fn prompt_yes_no(label: &str, default: bool) -> ResipResult<bool> {
    let suffix = if default { "Y/n" } else { "y/N" };
    loop {
        print!("{label} [{suffix}]: ");
        io::stdout().flush().map_err(ResipError::FlushStdout)?;
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(ResipError::ReadStdin)?;
        let value = input.trim().to_lowercase();
        if value.is_empty() {
            return Ok(default);
        }
        match value.as_str() {
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => eprintln!("Please answer y or n."),
        }
    }
}

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

    if expanded.is_absolute() {
        if let Ok(rest) = expanded.strip_prefix(&home) {
            if rest.as_os_str().is_empty() {
                return Ok("~".to_string());
            }
            return Ok(format!("~/{}", rest.to_string_lossy().replace('\\', "/")));
        }
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

fn absolute_dir(dir: &Path) -> ResipResult<PathBuf> {
    if dir.is_absolute() {
        return Ok(dir.to_path_buf());
    }

    Ok(env::current_dir()
        .map_err(ResipError::CurrentDir)?
        .join(dir))
}

fn sanitize_file_stem(value: &str) -> String {
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

pub fn home_dir() -> ResipResult<PathBuf> {
    directories::UserDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .ok_or(ResipError::HomeDirUnavailable)
}

pub fn is_port_available(host: &str, port: u16) -> bool {
    TcpListener::bind((host, port)).is_ok()
}

pub fn command_exists(name: &str) -> bool {
    let Some(paths) = env::var_os("PATH") else {
        return false;
    };

    let candidates = if cfg!(windows) {
        vec![
            format!("{name}.exe"),
            format!("{name}.cmd"),
            format!("{name}.bat"),
            name.to_string(),
        ]
    } else {
        vec![name.to_string()]
    };

    env::split_paths(&paths).any(|dir| {
        candidates
            .iter()
            .any(|candidate| dir.join(candidate).is_file())
    })
}
