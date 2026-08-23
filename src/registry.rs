use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("{0}")]
    Recipe(String),
    #[error("Steam download is not supported; mark lutris.existing_only")]
    SteamSourceNotSupported,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Gog,
    Steam,
    Custom,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Default)]
pub struct LutrisPin {
    #[serde(default)]
    pub installer_id: String,
    #[serde(default)]
    pub installer_slug: String,
    #[serde(default)]
    pub runner: String,
    #[serde(default = "default_proton")]
    pub proton: String,
    #[serde(default)]
    pub existing_only: bool,
    #[serde(default)]
    pub directory_template: String,
}

fn default_proton() -> String {
    "ge-latest".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Default)]
pub struct SteamGridDbPin {
    #[serde(default)]
    pub game_id: i64,
    #[serde(default)]
    pub landscape_id: i64,
    #[serde(default)]
    pub capsule_id: i64,
    #[serde(default)]
    pub hero_id: i64,
    #[serde(default)]
    pub logo_id: i64,
    #[serde(default)]
    pub icon_id: i64,
}

impl SteamGridDbPin {
    pub fn complete(&self) -> bool {
        [
            self.game_id,
            self.landscape_id,
            self.capsule_id,
            self.hero_id,
            self.logo_id,
            self.icon_id,
        ]
        .iter()
        .all(|value| *value > 0)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Default)]
pub struct LudusaviPin {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub game_key: String,
    #[serde(default)]
    pub custom_games: Vec<String>,
    #[serde(default)]
    pub registry_portability_proven: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Default)]
pub struct LineagePin {
    pub import_root: Option<String>,
    pub import_generation: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Recipe {
    pub slug: String,
    pub title: String,
    pub source: Source,
    #[serde(default)]
    pub installable: bool,
    #[serde(default)]
    pub product_id: String,
    #[serde(default)]
    pub upstream: String,
    #[serde(default)]
    pub lutris: LutrisPin,
    #[serde(default)]
    pub steam_shortcut: bool,
    #[serde(default)]
    pub steamgriddb: SteamGridDbPin,
    #[serde(default)]
    pub ludusavi: LudusaviPin,
    #[serde(default)]
    pub lineage: LineagePin,
    #[serde(default)]
    pub notes: Vec<String>,
}

pub fn parse_recipe(text: &str) -> Result<Recipe, Error> {
    let recipe: Recipe =
        serde_yaml::from_str(text).map_err(|error| Error::Recipe(error.to_string()))?;
    validate_slug(&recipe.slug)?;
    if recipe.installable {
        require_install_pins(&recipe)?;
    }
    Ok(recipe)
}

fn validate_slug(slug: &str) -> Result<(), Error> {
    if slug
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
        && !slug.is_empty()
    {
        Ok(())
    } else {
        Err(Error::Recipe("slug must match ^[a-z0-9-]+$".to_string()))
    }
}

fn require_install_pins(recipe: &Recipe) -> Result<(), Error> {
    match recipe.source {
        Source::Gog | Source::Steam if recipe.product_id.trim().is_empty() => {
            return Err(Error::Recipe(
                "product_id is required for installable store titles".to_string(),
            ));
        }
        Source::Custom if recipe.upstream.trim().is_empty() => {
            return Err(Error::Recipe(
                "upstream is required for installable custom titles".to_string(),
            ));
        }
        _ => {}
    }
    if !recipe.lutris.existing_only && recipe.lutris.installer_slug.trim().is_empty() {
        return Err(Error::Recipe(
            "lutris.installer_slug is required unless existing_only".to_string(),
        ));
    }
    if matches!(recipe.source, Source::Gog | Source::Custom)
        && recipe.lutris.runner.trim().is_empty()
    {
        return Err(Error::Recipe(
            "lutris.runner is required when installable".to_string(),
        ));
    }
    if recipe.ludusavi.name.trim().is_empty() {
        return Err(Error::Recipe(
            "ludusavi.name is required when installable".to_string(),
        ));
    }
    if matches!(recipe.source, Source::Gog | Source::Custom)
        && recipe.steam_shortcut
        && !recipe.steamgriddb.complete()
    {
        return Err(Error::Recipe(
            "steamgriddb IDs are required when steam_shortcut is true".to_string(),
        ));
    }
    if matches!(recipe.source, Source::Steam) && recipe.steam_shortcut {
        return Err(Error::Recipe(
            "official Steam titles do not use a Lutris Steam shortcut".to_string(),
        ));
    }
    Ok(())
}

impl Recipe {
    pub fn assert_installable(&self) -> Result<(), Error> {
        if matches!(self.source, Source::Steam) && !self.lutris.existing_only {
            return Err(Error::SteamSourceNotSupported);
        }
        if !self.installable {
            return Err(Error::Recipe(format!(
                "{} is not marked installable",
                self.slug
            )));
        }
        require_install_pins(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn example_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("registry/games/example-game.yaml")
    }

    #[test]
    fn parses_placeholder_recipe() {
        let text = std::fs::read_to_string(example_path()).unwrap();
        let recipe = parse_recipe(&text).unwrap();
        assert_eq!(recipe.slug, "example-game");
        assert!(!recipe.installable);
        assert!(!recipe.steam_shortcut);
    }

    #[test]
    fn installable_gog_recipe_requires_pins() {
        let doc = r#"
slug: example-installable
title: Example Installable
source: gog
installable: true
product_id: "1"
lutris:
  existing_only: true
  installer_slug: example
  runner: wine
steam_shortcut: true
steamgriddb:
  game_id: 1
  landscape_id: 1
  capsule_id: 1
  hero_id: 1
  logo_id: 1
  icon_id: 1
ludusavi:
  name: Example Installable
"#;
        let recipe = parse_recipe(doc).unwrap();
        assert!(recipe.steamgriddb.complete());
        recipe.assert_installable().unwrap();
    }

    #[test]
    fn steam_download_is_not_supported() {
        let doc = r#"
slug: some-steam-game
title: Some Steam Game
source: steam
installable: true
product_id: "123"
lutris:
  existing_only: false
  installer_slug: unused
ludusavi:
  name: Some Steam Game
"#;
        let recipe = parse_recipe(doc).unwrap();
        assert!(matches!(
            recipe.assert_installable(),
            Err(Error::SteamSourceNotSupported)
        ));
    }

    #[test]
    fn steam_existing_only_is_installable() {
        let doc = r#"
slug: baldurs-gate-3
title: Baldur's Gate 3
source: steam
installable: true
product_id: "1086940"
lutris:
  existing_only: true
ludusavi:
  name: Baldur's Gate 3
"#;
        let recipe = parse_recipe(doc).unwrap();
        recipe.assert_installable().unwrap();
    }

    #[test]
    fn invalid_slug_rejected() {
        let doc = r#"
slug: Monster Train
title: Monster Train
source: gog
installable: false
"#;
        assert!(parse_recipe(doc).is_err());
    }
}
