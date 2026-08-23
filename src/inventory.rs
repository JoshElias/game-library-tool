use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GamingHost {
    pub name: String,
    pub alias: String,
    pub desktop_user: String,
    pub xdg_config_home: String,
    pub xdg_data_home: String,
    pub xdg_state_home: String,
    pub xdg_cache_home: String,
    pub game_library_root: String,
    pub gog_library_root: String,
    pub gog_installation_roots: std::collections::BTreeMap<String, String>,
}

impl GamingHost {
    pub fn rendered_gog_directory(&self, slug: &str) -> String {
        format!("{}/{}", self.gog_library_root.trim_end_matches('/'), slug)
    }

    pub fn installation_root(&self, slug: &str) -> Option<&str> {
        self.gog_installation_roots.get(slug).map(String::as_str)
    }
}

pub fn require_absolute_path(name: &str, value: &str) -> Result<String, Error> {
    if value.is_empty() {
        return Err(Error::Message(format!("{name} is missing")));
    }
    if !value.starts_with('/') {
        return Err(Error::Message(format!("{name} must be an absolute path")));
    }
    Ok(value.to_string())
}

pub fn local_host_named(name: &str) -> Result<GamingHost, Error> {
    let user =
        crate::paths::current_username().ok_or_else(|| Error::Message("USER is unset".into()))?;
    let hostname = crate::paths::current_hostname()
        .ok_or_else(|| Error::Message("/etc/hostname is missing".into()))?;
    if name != hostname {
        return Err(Error::Message(format!(
            "{name} is not this host ({hostname}); add it to the hosts file"
        )));
    }
    Ok(local_host(name, &user))
}

fn local_only_hosts() -> Result<Vec<GamingHost>, Error> {
    let hostname = crate::paths::current_hostname()
        .ok_or_else(|| Error::Message("/etc/hostname is missing".into()))?;
    Ok(vec![local_host_named(&hostname)?])
}

pub fn local_host(name: &str, user: &str) -> GamingHost {
    let home = format!("/home/{user}");
    let data = format!("{home}/.local/share");
    GamingHost {
        name: name.to_string(),
        alias: "local".into(),
        desktop_user: user.to_string(),
        xdg_config_home: format!("{home}/.config"),
        xdg_data_home: data.clone(),
        xdg_state_home: format!("{home}/.local/state"),
        xdg_cache_home: format!("{home}/.cache"),
        game_library_root: format!("{data}/games"),
        gog_library_root: format!("{data}/games/gog"),
        gog_installation_roots: std::collections::BTreeMap::new(),
    }
}

pub fn repo_root() -> PathBuf {
    crate::paths::inventory_repo(std::env::current_dir().ok().as_deref())
}

pub fn exists(path: &Path) -> bool {
    path.exists()
}

pub fn load_hosts() -> Result<Vec<GamingHost>, Error> {
    if let Some(path) = crate::config::resolved().hosts_file {
        return crate::hosts_file::read_hosts_file(&path);
    }
    local_only_hosts()
}

pub fn load_host(name: &str) -> Result<GamingHost, Error> {
    if let Some(path) = crate::config::resolved().hosts_file {
        let hosts = crate::hosts_file::read_hosts_file(&path)?;
        return hosts
            .into_iter()
            .find(|host| host.name == name)
            .ok_or_else(|| Error::Message(format!("{name} is not in the hosts file")));
    }
    local_host_named(name)
}
