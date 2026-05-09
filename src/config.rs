use crate::{
    error::{ResipError, ResipResult},
    utils,
};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub name: String,
    pub ssh_host: String,
    pub ssh_user: String,
    pub ssh_port: u16,
    pub identity_file: String,
    pub local_tunnel_host: String,
    pub local_tunnel_port: u16,
    pub remote_proxy_host: String,
    pub remote_proxy_port: u16,
    pub local_clash_port: u16,
    pub clash_output_path: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            name: "resip-server".to_string(),
            ssh_host: String::new(),
            ssh_user: "ubuntu".to_string(),
            ssh_port: 22,
            identity_file: "~/.ssh/id_ed25519".to_string(),
            local_tunnel_host: "127.0.0.1".to_string(),
            local_tunnel_port: 7891,
            remote_proxy_host: "127.0.0.1".to_string(),
            remote_proxy_port: 7890,
            local_clash_port: 7890,
            clash_output_path: "~/Downloads/resip-resip-server.yaml".to_string(),
        }
    }
}

impl Config {
    pub fn interactive(server_ip: Option<String>, output: Option<String>) -> ResipResult<Self> {
        let mut config = Self::default();
        config.identity_file = utils::detect_default_identity_file();
        config.clash_output_path = match output {
            Some(path) => path,
            None => utils::default_clash_output_path(&config.name)?,
        };
        config.ssh_host = match server_ip {
            Some(value) => value,
            None => utils::prompt_required("Server IP")?,
        };
        config.ssh_user = utils::prompt_default("SSH user", &config.ssh_user)?;
        config.ssh_port = utils::prompt_default("SSH port", &config.ssh_port.to_string())?
            .parse::<u16>()
            .map_err(ResipError::InvalidSshPort)?;
        if !utils::expand_tilde(&config.identity_file)?.is_file() {
            println!(
                "No existing SSH private key found in ~/.ssh; defaulting to {}.",
                config.identity_file
            );
        }
        config.identity_file = utils::collapse_home(&utils::prompt_default(
            "Identity file",
            &config.identity_file,
        )?)?;
        Ok(config)
    }

    pub fn path() -> ResipResult<PathBuf> {
        let dirs = ProjectDirs::from("", "", "resip").ok_or(ResipError::ConfigDirUnavailable)?;
        Ok(dirs.config_dir().join("config.json"))
    }

    pub fn load() -> ResipResult<Self> {
        let path = Self::path()?;
        let contents = fs::read_to_string(&path).map_err(|source| ResipError::ReadFile {
            path: path.display().to_string(),
            source,
        })?;
        serde_json::from_str(&contents).map_err(|source| ResipError::ParseJson {
            path: path.display().to_string(),
            source,
        })
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
}
