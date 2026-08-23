use std::path::{Path, PathBuf};
use std::process::Command;

use minijinja::Environment;
use serde_json::Value;

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

pub trait InventoryLoader {
    fn load(&self, args: &[&str]) -> Result<Value, Error>;
}

pub struct AnsibleInventory {
    pub repo_root: PathBuf,
}

impl Default for AnsibleInventory {
    fn default() -> Self {
        Self {
            repo_root: crate::paths::inventory_repo(std::env::current_dir().ok().as_deref()),
        }
    }
}

impl InventoryLoader for AnsibleInventory {
    fn load(&self, args: &[&str]) -> Result<Value, Error> {
        let output = Command::new("ansible-inventory")
            .current_dir(&self.repo_root)
            .args([
                "-i",
                "inventory/daemon-fleet.yml",
                "-i",
                "inventory/hosts.yml",
            ])
            .args(args)
            .output()
            .map_err(|error| Error::Message(error.to_string()))?;
        if !output.status.success() {
            return Err(Error::Message(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        serde_json::from_slice(&output.stdout).map_err(|error| Error::Message(error.to_string()))
    }
}

const REQUIRED_PATHS: &[&str] = &[
    "gaming_workstations_xdg_config_home",
    "gaming_workstations_xdg_data_home",
    "gaming_workstations_xdg_state_home",
    "gaming_workstations_xdg_cache_home",
    "gaming_workstations_game_library_root",
    "gaming_workstations_gog_library_root",
];

fn group_hosts(listing: &Value, group: &str) -> Vec<String> {
    listing
        .get(group)
        .and_then(|value| value.get("hosts"))
        .and_then(Value::as_array)
        .map(|hosts| {
            hosts
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn render_vars(raw: &Value) -> Result<Value, Error> {
    let mut context = raw.clone();
    if context
        .get("workstation_user_home")
        .and_then(Value::as_str)
        .is_none_or(|value| value.is_empty())
    {
        if let Some(user) = context.get("workstation_user").and_then(Value::as_str) {
            context["workstation_user_home"] = Value::String(format!("/home/{user}"));
        }
    }
    let env = Environment::new();
    for _ in 0..8 {
        let (next, changed) = render_value(&env, context.clone(), &context)?;
        context = next;
        if !changed {
            break;
        }
    }
    Ok(context)
}

fn render_value(env: &Environment, value: Value, context: &Value) -> Result<(Value, bool), Error> {
    match value {
        Value::String(text) => {
            let text = text.replace(
                "{{ workstation_user_home | default('/home/' ~ workstation_user) }}",
                "{{ workstation_user_home }}",
            );
            if text.contains("{{") {
                let rendered = env
                    .render_str(&text, context)
                    .map_err(|error| Error::Message(error.to_string()))?;
                Ok((Value::String(rendered), true))
            } else {
                Ok((Value::String(text), false))
            }
        }
        Value::Object(map) => {
            let mut changed = false;
            let mut next = serde_json::Map::new();
            for (key, child) in map {
                let (rendered, child_changed) = render_value(env, child, context)?;
                changed |= child_changed;
                next.insert(key, rendered);
            }
            Ok((Value::Object(next), changed))
        }
        Value::Array(items) => {
            let mut changed = false;
            let mut next = Vec::new();
            for child in items {
                let (rendered, child_changed) = render_value(env, child, context)?;
                changed |= child_changed;
                next.push(rendered);
            }
            Ok((Value::Array(next), changed))
        }
        other => Ok((other, false)),
    }
}

pub fn require_absolute_path(name: &str, value: &str) -> Result<String, Error> {
    require_absolute(name, Some(value))
}

fn require_absolute(name: &str, value: Option<&str>) -> Result<String, Error> {
    let value = value
        .filter(|text| !text.is_empty())
        .ok_or_else(|| Error::Message(format!("{name} is missing")))?;
    if value.contains("{{") || value.contains("}}") {
        return Err(Error::Message(format!(
            "{name} still contains unresolved Jinja"
        )));
    }
    if !value.starts_with('/') {
        return Err(Error::Message(format!("{name} must be an absolute path")));
    }
    Ok(value.to_string())
}

pub fn resolve_hosts(loader: &dyn InventoryLoader) -> Result<Vec<GamingHost>, Error> {
    if !crate::paths::inventory_present(&repo_root()) {
        return local_only_hosts();
    }
    let listing = loader.load(&["--list"])?;
    let mut gaming = group_hosts(&listing, "gaming_workstations");
    let access = group_hosts(&listing, "game_library_ssh_access");
    gaming.sort();
    gaming
        .into_iter()
        .filter(|name| access.contains(name))
        .map(|name| resolve_host(&name, loader, Some(&listing)))
        .collect()
}

pub fn resolve_host(
    name: &str,
    loader: &dyn InventoryLoader,
    listing: Option<&Value>,
) -> Result<GamingHost, Error> {
    if !crate::paths::inventory_present(&repo_root()) {
        return local_host_named(name);
    }
    let owned = if listing.is_none() {
        Some(loader.load(&["--list"])?)
    } else {
        None
    };
    let listing = listing.unwrap_or_else(|| owned.as_ref().unwrap());
    let gaming = group_hosts(listing, "gaming_workstations");
    let access = group_hosts(listing, "game_library_ssh_access");
    if !gaming.iter().any(|host| host == name) {
        return Err(Error::Message(format!(
            "{name} is not a production gaming_workstations host"
        )));
    }
    if !access.iter().any(|host| host == name) {
        return Err(Error::Message(format!(
            "{name} has no approved game-library SSH route"
        )));
    }
    let host_vars = render_vars(&loader.load(&["--host", name])?)?;
    let desktop_user = host_vars
        .get("workstation_user")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::Message("workstation_user is missing".to_string()))?
        .to_string();
    let mut paths = std::collections::BTreeMap::new();
    for key in REQUIRED_PATHS {
        paths.insert(
            *key,
            require_absolute(key, host_vars.get(*key).and_then(Value::as_str))?,
        );
    }
    let mut roots = std::collections::BTreeMap::new();
    if let Some(object) = host_vars
        .get("gaming_workstations_gog_installation_roots")
        .and_then(Value::as_object)
    {
        for (slug, value) in object {
            roots.insert(
                slug.clone(),
                require_absolute(
                    &format!("gaming_workstations_gog_installation_roots.{slug}"),
                    value.as_str(),
                )?,
            );
        }
    }
    Ok(GamingHost {
        name: name.to_string(),
        alias: format!("{name}-games"),
        desktop_user,
        xdg_config_home: paths["gaming_workstations_xdg_config_home"].clone(),
        xdg_data_home: paths["gaming_workstations_xdg_data_home"].clone(),
        xdg_state_home: paths["gaming_workstations_xdg_state_home"].clone(),
        xdg_cache_home: paths["gaming_workstations_xdg_cache_home"].clone(),
        game_library_root: paths["gaming_workstations_game_library_root"].clone(),
        gog_library_root: paths["gaming_workstations_gog_library_root"].clone(),
        gog_installation_roots: roots,
    })
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
    resolve_hosts(&AnsibleInventory::default())
}

pub fn load_host(name: &str) -> Result<GamingHost, Error> {
    if let Some(path) = crate::config::resolved().hosts_file {
        let hosts = crate::hosts_file::read_hosts_file(&path)?;
        return hosts
            .into_iter()
            .find(|host| host.name == name)
            .ok_or_else(|| Error::Message(format!("{name} is not in the hosts file")));
    }
    resolve_host(name, &AnsibleInventory::default(), None)
}
