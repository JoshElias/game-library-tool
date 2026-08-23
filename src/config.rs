use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Deserialize;

use crate::ssh::DEFAULT_SSH;

static ENV_LOCK: Mutex<()> = Mutex::new(());

pub const DEFAULT_LINEAGE_REMOTE: &str = "ludusavi";
pub const DEFAULT_SHARED_GOG: &str = "";

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct FileConfig {
    #[serde(default)]
    pub ssh_helper: Option<String>,
    #[serde(default)]
    pub hosts: Option<String>,
    #[serde(default)]
    pub lineage_remote: Option<String>,
    #[serde(default)]
    pub shared_gog_root: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedConfig {
    pub ssh_helper: PathBuf,
    pub hosts_file: Option<PathBuf>,
    pub lineage_remote: String,
    pub shared_gog_root: String,
}

pub fn with_locked_env<R>(pairs: &[(&str, Option<&str>)], body: impl FnOnce() -> R) -> R {
    let _guard = ENV_LOCK.lock().unwrap();
    let old: Vec<(String, Option<String>)> = pairs
        .iter()
        .map(|(key, _)| ((*key).to_string(), std::env::var(key).ok()))
        .collect();
    for (key, value) in pairs {
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
    let result = body();
    for (key, value) in old {
        match value {
            Some(value) => std::env::set_var(&key, value),
            None => std::env::remove_var(&key),
        }
    }
    result
}

pub fn config_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("GAME_LIBRARY_CONFIG") {
        return Some(PathBuf::from(path));
    }
    let home = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| PathBuf::from(home).join(".config"))
        })?;
    let path = home.join("game-library/config.yaml");
    path.is_file().then_some(path)
}

pub fn load_file(path: &Path) -> Result<FileConfig, String> {
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_yaml::from_str(&text).map_err(|error| error.to_string())
}

fn normalize_shared_gog(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.ends_with('/') {
        trimmed.to_string()
    } else {
        format!("{trimmed}/")
    }
}

pub fn resolved() -> ResolvedConfig {
    let file = config_path().and_then(|path| load_file(&path).ok());
    let ssh_helper = std::env::var("GAME_LIBRARY_SSH")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| file.as_ref().and_then(|cfg| cfg.ssh_helper.clone()))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SSH));
    let hosts_file = std::env::var("GAME_LIBRARY_HOSTS")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| file.as_ref().and_then(|cfg| cfg.hosts.clone()))
        .map(PathBuf::from);
    let lineage_remote = std::env::var("GAME_LIBRARY_LINEAGE_REMOTE")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| file.as_ref().and_then(|cfg| cfg.lineage_remote.clone()))
        .unwrap_or_else(|| DEFAULT_LINEAGE_REMOTE.to_string());
    let shared_gog_root = std::env::var("GAME_LIBRARY_SHARED_GOG")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| file.as_ref().and_then(|cfg| cfg.shared_gog_root.clone()))
        .map(|value| normalize_shared_gog(&value))
        .unwrap_or_else(|| DEFAULT_SHARED_GOG.to_string());
    ResolvedConfig {
        ssh_helper,
        hosts_file,
        lineage_remote,
        shared_gog_root,
    }
}
