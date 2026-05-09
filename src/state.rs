use crate::error::{ResipError, ResipResult};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub pid: u32,
    pub started_at: String,
    pub local_tunnel_host: String,
    pub local_tunnel_port: u16,
    pub server: String,
    #[serde(default)]
    pub forward: Option<String>,
    #[serde(default)]
    pub destination: Option<String>,
}

impl State {
    pub fn path() -> ResipResult<PathBuf> {
        let dirs = ProjectDirs::from("", "", "resip").ok_or(ResipError::StateDirUnavailable)?;
        Ok(dirs.data_local_dir().join("state.json"))
    }

    pub fn load_optional() -> ResipResult<Option<Self>> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(None);
        }
        let contents = fs::read_to_string(&path).map_err(|source| ResipError::ReadFile {
            path: path.display().to_string(),
            source,
        })?;
        let state = serde_json::from_str(&contents).map_err(|source| ResipError::ParseJson {
            path: path.display().to_string(),
            source,
        })?;
        Ok(Some(state))
    }

    pub fn save(&self) -> ResipResult<()> {
        let path = Self::path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| ResipError::CreateDirectory {
                path: parent.display().to_string(),
                source,
            })?;
        }
        let contents = serde_json::to_string_pretty(self).map_err(ResipError::SerializeJson)?;
        fs::write(&path, contents).map_err(|source| ResipError::WriteFile {
            path: path.display().to_string(),
            source,
        })
    }

    pub fn remove() -> ResipResult<()> {
        let path = Self::path()?;
        if path.exists() {
            fs::remove_file(&path).map_err(|source| ResipError::RemoveFile {
                path: path.display().to_string(),
                source,
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::State;

    #[test]
    fn loads_state_written_by_older_versions() {
        let json = r#"{
            "pid": 123,
            "started_at": "2026-05-10T00:00:00Z",
            "local_tunnel_host": "127.0.0.1",
            "local_tunnel_port": 7891,
            "server": "ubuntu@example.com:22"
        }"#;

        let state: State = serde_json::from_str(json).unwrap();

        assert_eq!(state.pid, 123);
        assert_eq!(state.forward, None);
        assert_eq!(state.destination, None);
    }
}
