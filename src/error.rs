use std::num::ParseIntError;
use thiserror::Error;

pub type ResipResult<T> = std::result::Result<T, ResipError>;

#[derive(Debug, Error)]
pub enum ResipError {
    #[error("failed to locate user configuration directory")]
    ConfigDirUnavailable,

    #[error("failed to locate user state directory")]
    StateDirUnavailable,

    #[error("failed to locate home directory")]
    HomeDirUnavailable,

    #[error("SSH port must be a number between 1 and 65535")]
    InvalidSshPort(#[source] ParseIntError),

    #[error("failed to create directory: {path}")]
    CreateDirectory {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to read file: {path}")]
    ReadFile {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to write file: {path}")]
    WriteFile {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to remove file: {path}")]
    RemoveFile {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse JSON file: {path}")]
    ParseJson {
        path: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("failed to serialize JSON")]
    SerializeJson(#[source] serde_json::Error),

    #[error("failed to serialize Clash YAML")]
    SerializeYaml(#[source] serde_yml::Error),

    #[error("ssh was not found in PATH")]
    SshNotFound,

    #[error("local tunnel port is already in use: {host}:{port}")]
    PortInUse { host: String, port: u16 },

    #[error("failed to start ssh tunnel")]
    StartSsh(#[source] std::io::Error),

    #[error("ssh tunnel exited immediately: {reason}")]
    SshExitedImmediately { reason: String },

    #[error("ssh tunnel started, but local forward did not become active: {host}:{port}")]
    SshForwardNotReady { host: String, port: u16 },

    #[error("failed to run command: {program}")]
    RunCommand {
        program: &'static str,
        #[source]
        source: std::io::Error,
    },

    #[error("command failed: {program}")]
    CommandFailed { program: &'static str },

    #[error("failed to get current directory")]
    CurrentDir(#[source] std::io::Error),

    #[error("failed to open directory: {path}")]
    OpenDirectory {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("system file manager failed to open directory: {path}")]
    FileManagerFailed { path: String },

    #[error("Clash config was generated, but failed to open directory: {0}")]
    GeneratedButOpenFailed(#[source] anyhow::Error),

    #[error("failed to create HTTP proxy: {url}")]
    CreateHttpProxy {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("failed to build HTTP client")]
    BuildHttpClient(#[source] reqwest::Error),

    #[error("test request through local Clash failed")]
    ProxyTest(#[source] reqwest::Error),

    #[error("failed to flush stdout")]
    FlushStdout(#[source] std::io::Error),

    #[error("failed to read from stdin")]
    ReadStdin(#[source] std::io::Error),
}
