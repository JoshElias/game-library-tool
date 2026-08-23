use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::registry::{parse_recipe, Error as RegistryError, Recipe};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Registry(#[from] RegistryError),
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Message(String),
}

pub struct RegistryStore {
    pub root: PathBuf,
}

impl RegistryStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn default_root() -> PathBuf {
        crate::paths::registry_root()
    }

    pub fn path_for(&self, slug: &str) -> Result<PathBuf, Error> {
        crate::registry::parse_recipe(&format!(
            "slug: {slug}\ntitle: x\nsource: gog\ninstallable: false\n"
        ))
        .map_err(|_| RegistryError::Recipe("slug must match ^[a-z0-9-]+$".into()))?;
        let path = self.root.join(format!("{slug}.yaml"));
        let root = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone());
        if path
            .parent()
            .is_some_and(|parent| parent.starts_with(&root) || parent == root)
            || !self.root.exists()
        {
            return Ok(path);
        }
        Err(Error::Message(
            "recipe path escapes the registry root".to_string(),
        ))
    }

    pub fn list(&self) -> Result<Vec<Recipe>, Error> {
        if !self.root.is_dir() {
            return Ok(Vec::new());
        }
        let mut slugs = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("yaml") {
                if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
                    slugs.push(stem.to_string());
                }
            }
        }
        slugs.sort();
        slugs.into_iter().map(|slug| self.show(&slug)).collect()
    }

    pub fn show(&self, slug: &str) -> Result<Recipe, Error> {
        let path = self.path_for(slug)?;
        if !path.is_file() {
            return Err(Error::Message(format!("unknown game: {slug}")));
        }
        Ok(parse_recipe(&fs::read_to_string(path)?)?)
    }

    pub fn add(&self, recipe: &Recipe, force: bool) -> Result<Recipe, Error> {
        fs::create_dir_all(&self.root)?;
        let path = self.path_for(&recipe.slug)?;
        if path.exists() && !force {
            return Err(Error::Message(format!("{} already exists", recipe.slug)));
        }
        fs::write(
            path,
            serde_yaml::to_string(recipe).map_err(|error| Error::Message(error.to_string()))?,
        )?;
        Ok(recipe.clone())
    }

    pub fn remove(&self, slug: &str, confirm: &str) -> Result<(), Error> {
        if confirm != slug {
            return Err(Error::Message(
                "remove confirmation must match the slug".to_string(),
            ));
        }
        let path = self.path_for(slug)?;
        if !path.is_file() {
            return Err(Error::Message(format!("unknown game: {slug}")));
        }
        fs::remove_file(path)?;
        Ok(())
    }
}

const ADD_FIELDS: &[&str] = &[
    "slug",
    "title",
    "source",
    "product_id",
    "upstream",
    "installer_slug",
    "installer_id",
    "runner",
    "proton",
    "existing_only",
    "directory_template",
    "steam_shortcut",
    "game_id",
    "landscape_id",
    "capsule_id",
    "hero_id",
    "logo_id",
    "icon_id",
    "ludusavi_name",
    "game_key",
];

pub fn prompt_new_recipe<R, W>(mut reader: R, mut writer: W) -> Result<Recipe, Error>
where
    R: FnMut(&str) -> io::Result<String>,
    W: Write,
{
    let mut answers = std::collections::BTreeMap::new();
    for field in ADD_FIELDS {
        let value = reader(&format!("{field}: "))?;
        answers.insert(*field, value.trim().to_string());
    }
    let steam_shortcut = as_bool(
        answers
            .get("steam_shortcut")
            .map(String::as_str)
            .unwrap_or(""),
    );
    let existing_only = as_bool(
        answers
            .get("existing_only")
            .map(String::as_str)
            .unwrap_or(""),
    );
    let mut doc = format!(
        "slug: {slug}\ntitle: {title}\nsource: {source}\ninstallable: false\nproduct_id: \"{product}\"\nupstream: {upstream}\nlutris:\n  installer_id: \"{installer_id}\"\n  installer_slug: \"{installer_slug}\"\n  runner: {runner}\n  proton: {proton}\n  existing_only: {existing_only}\n  directory_template: \"{directory}\"\nsteam_shortcut: {steam_shortcut}\nsteamgriddb:\n  game_id: {game_id}\n  landscape_id: {landscape_id}\n  capsule_id: {capsule_id}\n  hero_id: {hero_id}\n  logo_id: {logo_id}\n  icon_id: {icon_id}\nludusavi:\n  name: {ludusavi_name}\n  game_key: {game_key}\n  custom_games: []\n  registry_portability_proven: false\n",
        slug = answers["slug"],
        title = answers["title"],
        source = answers["source"],
        product = answers["product_id"],
        upstream = answers["upstream"],
        installer_id = answers["installer_id"],
        installer_slug = answers["installer_slug"],
        runner = if answers["runner"].is_empty() { "wine" } else { &answers["runner"] },
        proton = if answers["proton"].is_empty() { "ge-latest" } else { &answers["proton"] },
        existing_only = existing_only,
        directory = answers["directory_template"],
        steam_shortcut = steam_shortcut,
        game_id = as_int(&answers["game_id"]),
        landscape_id = as_int(&answers["landscape_id"]),
        capsule_id = as_int(&answers["capsule_id"]),
        hero_id = as_int(&answers["hero_id"]),
        logo_id = as_int(&answers["logo_id"]),
        icon_id = as_int(&answers["icon_id"]),
        ludusavi_name = answers["ludusavi_name"],
        game_key = answers["game_key"],
    );
    let mut recipe = parse_recipe(&doc)?;
    let installable_doc = doc.replacen("installable: false", "installable: true", 1);
    if parse_recipe(&installable_doc).is_ok() {
        let mark = reader("mark installable? ")?;
        if matches!(mark.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            recipe.installable = true;
            doc = installable_doc;
            recipe = parse_recipe(&doc)?;
        }
    } else {
        writeln!(writer, "saved as not installable")?;
    }
    let _ = installable_doc;
    Ok(recipe)
}

fn as_bool(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "y"
    )
}

fn as_int(value: &str) -> i64 {
    value.parse().unwrap_or(0)
}

pub fn store_from_env() -> RegistryStore {
    RegistryStore::new(RegistryStore::default_root())
}

pub fn path_exists(path: &Path) -> bool {
    path.exists()
}
