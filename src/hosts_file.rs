use std::path::Path;

use serde::Deserialize;

use crate::inventory::{require_absolute_path, GamingHost};

#[derive(Debug, Deserialize)]
struct HostsDocument {
    hosts: Vec<HostEntry>,
}

#[derive(Debug, Deserialize)]
struct HostEntry {
    name: String,
    user: String,
    #[serde(default)]
    ssh: String,
    xdg_config_home: String,
    xdg_data_home: String,
    xdg_state_home: String,
    xdg_cache_home: String,
    game_library_root: String,
    gog_library_root: String,
    #[serde(default)]
    gog_installation_roots: std::collections::BTreeMap<String, String>,
}

pub fn read_hosts_file(path: &Path) -> Result<Vec<GamingHost>, crate::inventory::Error> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| crate::inventory::Error::Message(error.to_string()))?;
    parse_hosts_yaml(&text)
}

pub fn parse_hosts_yaml(text: &str) -> Result<Vec<GamingHost>, crate::inventory::Error> {
    let document: HostsDocument = serde_yaml::from_str(text)
        .map_err(|error| crate::inventory::Error::Message(error.to_string()))?;
    if document.hosts.is_empty() {
        return Err(crate::inventory::Error::Message(
            "hosts file has no hosts".into(),
        ));
    }
    document.hosts.into_iter().map(entry_to_host).collect()
}

fn entry_to_host(entry: HostEntry) -> Result<GamingHost, crate::inventory::Error> {
    if entry.name.trim().is_empty() {
        return Err(crate::inventory::Error::Message(
            "host name is missing".into(),
        ));
    }
    if entry.user.trim().is_empty() {
        return Err(crate::inventory::Error::Message(format!(
            "{} is missing user",
            entry.name
        )));
    }
    let alias = if entry.ssh.trim().is_empty() {
        "local".to_string()
    } else {
        entry.ssh.trim().to_string()
    };
    let mut roots = std::collections::BTreeMap::new();
    for (slug, path) in entry.gog_installation_roots {
        roots.insert(
            slug.clone(),
            require_absolute_path(
                &format!("{}.gog_installation_roots.{slug}", entry.name),
                &path,
            )?,
        );
    }
    Ok(GamingHost {
        name: entry.name.clone(),
        alias,
        desktop_user: entry.user,
        xdg_config_home: require_absolute_path(
            &format!("{}.xdg_config_home", entry.name),
            &entry.xdg_config_home,
        )?,
        xdg_data_home: require_absolute_path(
            &format!("{}.xdg_data_home", entry.name),
            &entry.xdg_data_home,
        )?,
        xdg_state_home: require_absolute_path(
            &format!("{}.xdg_state_home", entry.name),
            &entry.xdg_state_home,
        )?,
        xdg_cache_home: require_absolute_path(
            &format!("{}.xdg_cache_home", entry.name),
            &entry.xdg_cache_home,
        )?,
        game_library_root: require_absolute_path(
            &format!("{}.game_library_root", entry.name),
            &entry.game_library_root,
        )?,
        gog_library_root: require_absolute_path(
            &format!("{}.gog_library_root", entry.name),
            &entry.gog_library_root,
        )?,
        gog_installation_roots: roots,
    })
}
