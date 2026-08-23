use std::io::{self, Write};

use crate::art;
use crate::confirm::{require_go, ConfirmationError};
use crate::download;
use crate::inventory::GamingHost;
use crate::legacy::LegacyTreeError;
use crate::registry::{Error as RegistryError, Recipe};
use crate::status::{collect_status, Remote, StatusRecord};
use crate::wrap;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Confirm(#[from] ConfirmationError),
    #[error(transparent)]
    Legacy(#[from] LegacyTreeError),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error("{0}")]
    Message(String),
}

pub fn describe_plan(host: &GamingHost, recipe: &Recipe, status: &StatusRecord) -> String {
    format!(
        "host: {} ({}) user={}\ngame: {} / {} / {:?}\ninstaller: {}\ndirectory: {}\nsteam_shortcut: {}\nart_applied: {}\nludusavi: {}\nlegacy: {}\nalready_complete: {}\nrollback: remove only a new Lutris entry and new directory; never auth or other games\nverify: Lutris launch path, wrap prefix, lineage generation, pinned SteamGridDB journal",
        host.name,
        host.alias,
        host.desktop_user,
        recipe.slug,
        recipe.title,
        recipe.source,
        if recipe.lutris.installer_slug.is_empty() {
            "existing"
        } else {
            &recipe.lutris.installer_slug
        },
        status.canonical_directory,
        recipe.steam_shortcut,
        status.art_applied,
        recipe.ludusavi.name,
        status.legacy,
        status.complete,
    )
}

pub fn next_stage(recipe: &Recipe, status: &StatusRecord) -> Option<&'static str> {
    if status.complete {
        return None;
    }
    if status.legacy {
        return Some("legacy");
    }
    if !status.installed {
        return if recipe.lutris.existing_only {
            Some("existing_only")
        } else {
            Some("download")
        };
    }
    if recipe.steam_shortcut && !status.art_applied {
        return Some("art");
    }
    if !status.enrolled || !status.wrap_present {
        return Some("enroll");
    }
    Some("unknown")
}

pub fn install_game<R, W>(
    host: &GamingHost,
    recipe: &Recipe,
    remote: &dyn Remote,
    reader: R,
    writer: W,
    confirm: bool,
    import_root: Option<&str>,
) -> Result<String, Error>
where
    R: FnMut(&str) -> io::Result<String>,
    W: Write,
{
    recipe.assert_installable()?;
    let status = collect_status(host, recipe, remote);
    if confirm {
        require_go(reader, writer, &describe_plan(host, recipe, &status))?;
    }
    match next_stage(recipe, &status) {
        None => Ok(format!(
            "already complete: {} on {}",
            recipe.slug, host.name
        )),
        Some("legacy") => Err(LegacyTreeError {
            what: format!("{} on {}", recipe.slug, host.name),
            path: status
                .live_directory
                .clone()
                .or_else(|| host.installation_root(&recipe.slug).map(ToOwned::to_owned))
                .unwrap_or_default(),
        }
        .into()),
        Some("existing_only") => Err(Error::Message(format!(
            "{} is existing_only and is not installed on {}",
            recipe.slug, host.name
        ))),
        Some("download") => download::start_gog_download(host, recipe, &status, remote)
            .map_err(|error| Error::Message(error.to_string())),
        Some("art") => art::apply_pinned_art(host, recipe, &status, remote)
            .map_err(|error| Error::Message(error.to_string())),
        Some("enroll") => wrap::enroll_and_wrap(host, recipe, &status, remote, import_root)
            .map_err(|error| Error::Message(error.to_string())),
        Some(stage) => Err(Error::Message(format!(
            "{} is not complete on {}; remaining stage: {stage}",
            recipe.slug, host.name
        ))),
    }
}
