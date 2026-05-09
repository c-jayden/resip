use crate::utils;
use crate::{
    config::Config,
    error::{ResipError, ResipResult},
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
struct ClashConfig {
    port: u16,
    #[serde(rename = "allow-lan")]
    allow_lan: bool,
    mode: String,
    proxies: Vec<Proxy>,
    #[serde(rename = "proxy-groups")]
    proxy_groups: Vec<ProxyGroup>,
    rules: Vec<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_yaml::Value>,
}

#[derive(Debug, Serialize)]
struct Proxy {
    name: String,
    #[serde(rename = "type")]
    proxy_type: String,
    server: String,
    port: u16,
}

#[derive(Debug, Serialize)]
struct ProxyGroup {
    name: String,
    #[serde(rename = "type")]
    group_type: String,
    proxies: Vec<String>,
}

pub fn generate(config: &Config) -> ResipResult<PathBuf> {
    let path = utils::expand_tilde(&config.clash_output_path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ResipError::CreateDirectory {
            path: parent.display().to_string(),
            source,
        })?;
    }

    let proxy_name = config.name.clone();
    let clash = ClashConfig {
        port: config.local_clash_port,
        allow_lan: false,
        mode: "Rule".to_string(),
        proxies: vec![Proxy {
            name: proxy_name.clone(),
            proxy_type: "http".to_string(),
            server: config.local_tunnel_host.clone(),
            port: config.local_tunnel_port,
        }],
        proxy_groups: vec![ProxyGroup {
            name: "RESIP".to_string(),
            group_type: "select".to_string(),
            proxies: vec![proxy_name],
        }],
        rules: vec!["MATCH,RESIP".to_string()],
        extra: BTreeMap::new(),
    };

    let contents = serde_yaml::to_string(&clash).map_err(ResipError::SerializeYaml)?;
    fs::write(&path, contents).map_err(|source| ResipError::WriteFile {
        path: path.display().to_string(),
        source,
    })?;
    Ok(path)
}
