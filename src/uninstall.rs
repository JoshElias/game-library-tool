use std::io::{self, Write};

use crate::art::bundled_script;
use crate::confirm::{require_go, ConfirmationError};
use crate::inventory::GamingHost;
use crate::legacy::LegacyTreeError;
use crate::registry::{Recipe, Source};
use crate::status::{collect_status, run_args, Remote, StatusRecord};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Confirm(#[from] ConfirmationError),
    #[error(transparent)]
    Legacy(#[from] LegacyTreeError),
    #[error("{0}")]
    Message(String),
}

pub fn describe_plan(host: &GamingHost, recipe: &Recipe, status: &StatusRecord) -> String {
    format!(
        "UNINSTALL (files only)\nhost: {} ({}) user={}\ngame: {} / {} / {:?}\nlutris_id: {:?}\ndirectory: {}\ntrash: gio trash that directory only; do not empty Trash\nlutris: mark uninstalled; keep GOG/Steam library record\nlineage: keep generation {:?}; do not unenroll\nrecipe: keep\nrollback: restore the directory from Trash; Lutris stays uninstalled until reinstall\nverify: path gone, id absent from installed list, wrap still enrolled",
        host.name,
        host.alias,
        host.desktop_user,
        recipe.slug,
        recipe.title,
        recipe.source,
        status.lutris_id,
        status
            .live_directory
            .as_deref()
            .unwrap_or(&status.canonical_directory),
        status.generation,
    )
}

pub fn apply_argv(lutris_id: i64, directory: &str, slug: &str) -> Vec<String> {
    vec![
        "python3".into(),
        "/tmp/uninstall_lutris_game.py".into(),
        "--lutris-id".into(),
        lutris_id.to_string(),
        "--directory".into(),
        directory.into(),
        "--slug".into(),
        slug.into(),
        "--apply".into(),
    ]
}

pub fn lutris_bin_argv() -> Vec<String> {
    vec!["pgrep".into(), "-f".into(), "/usr/bin/lutris".into()]
}

pub fn lutris_sbin_argv() -> Vec<String> {
    vec!["pgrep".into(), "-f".into(), "/usr/sbin/lutris".into()]
}

fn under_gog_root(path: &str, root: &str) -> bool {
    let path = path.trim_end_matches('/');
    let root = root.trim_end_matches('/');
    path != root && path.starts_with(&format!("{root}/"))
}

fn lutris_running(remote: &dyn Remote) -> bool {
    remote.run(&["pgrep", "-x", "lutris"]).0 == 0
        || run_args(remote, &lutris_bin_argv()).0 == 0
        || run_args(remote, &lutris_sbin_argv()).0 == 0
}

fn steam_running(remote: &dyn Remote) -> bool {
    remote.run(&["pgrep", "-x", "steam"]).0 == 0
        || remote.run(&["pgrep", "-f", "steamwebhelper"]).0 == 0
}

fn refuse_preflight(
    host: &GamingHost,
    recipe: &Recipe,
    status: &StatusRecord,
    remote: &dyn Remote,
) -> Result<Option<String>, Error> {
    if matches!(recipe.source, Source::Steam) {
        return Err(Error::Message(format!(
            "refusing to uninstall Steam-source {} — that would remove the Steam app",
            recipe.slug
        )));
    }
    if status.legacy {
        return Err(LegacyTreeError {
            what: format!("{} on {}", recipe.slug, host.name),
            path: status
                .live_directory
                .clone()
                .unwrap_or_else(|| status.canonical_directory.clone()),
        }
        .into());
    }
    if status.import_journal_blocking {
        return Err(Error::Message(format!(
            "{} has a blocking import journal on {}",
            recipe.slug, host.name
        )));
    }
    if !status.installed {
        return Ok(Some(format!(
            "already uninstalled: {} on {}",
            recipe.slug, host.name
        )));
    }
    let Some(lutris_id) = status.lutris_id else {
        return Err(Error::Message(format!(
            "{} on {} has no Lutris id",
            recipe.slug, host.name
        )));
    };
    let Some(live) = status.live_directory.as_deref() else {
        return Err(Error::Message(format!(
            "{} on {} has no live directory",
            recipe.slug, host.name
        )));
    };
    if live != status.canonical_directory {
        return Err(Error::Message(format!(
            "live directory is not the canonical XDG tree: {live}"
        )));
    }
    if !under_gog_root(live, &host.gog_library_root) {
        return Err(Error::Message(format!(
            "refusing to trash a path outside the GOG library root: {live}"
        )));
    }
    if status.generation.as_deref().unwrap_or("").is_empty() {
        return Err(Error::Message(format!(
            "refusing to uninstall {} on {} without a retained lineage generation",
            recipe.slug, host.name
        )));
    }
    if lutris_running(remote) {
        return Err(Error::Message(
            "Lutris is running; close it and re-run uninstall".into(),
        ));
    }
    if recipe.steam_shortcut && steam_running(remote) {
        return Err(Error::Message(
            "Steam is running; close it before uninstalling a Steam-shortcut title".into(),
        ));
    }
    if remote.run(&["test", "-L", live]).0 == 0 {
        return Err(Error::Message(format!("refusing to trash symlink {live}")));
    }
    let _ = lutris_id;
    Ok(None)
}

pub fn uninstall_game<R, W>(
    host: &GamingHost,
    recipe: &Recipe,
    remote: &dyn Remote,
    reader: R,
    writer: W,
    confirm: bool,
) -> Result<String, Error>
where
    R: FnMut(&str) -> io::Result<String>,
    W: Write,
{
    let status = collect_status(host, recipe, remote);
    if confirm {
        require_go(reader, writer, &describe_plan(host, recipe, &status))?;
    }
    if let Some(message) = refuse_preflight(host, recipe, &status, remote)? {
        return Ok(message);
    }
    let lutris_id = status.lutris_id.expect("preflight required lutris id");
    let live = status
        .live_directory
        .as_deref()
        .expect("preflight required live directory");
    let generation = status
        .generation
        .clone()
        .expect("preflight required generation");
    let script = bundled_script("uninstall_lutris_game.py");
    let bytes = std::fs::read(&script).map_err(|error| Error::Message(error.to_string()))?;
    remote
        .write_file("/tmp/uninstall_lutris_game.py", &bytes)
        .map_err(Error::Message)?;
    let (code, stdout, stderr) = run_args(remote, &apply_argv(lutris_id, live, &recipe.slug));
    if code != 0 {
        return Err(Error::Message(format!(
            "uninstall failed on {}: {stdout}{stderr}",
            host.name
        )));
    }
    if remote.run(&["test", "-d", live]).0 == 0 {
        return Err(Error::Message(format!(
            "install directory still exists after trash: {live}"
        )));
    }
    let after = collect_status(host, recipe, remote);
    if after.installed {
        return Err(Error::Message(format!(
            "{} is still installed on {} after uninstall",
            recipe.slug, host.name
        )));
    }
    if after.generation.as_deref() != Some(generation.as_str()) || !after.enrolled {
        return Err(Error::Message(format!(
            "lineage changed after uninstalling {} on {}",
            recipe.slug, host.name
        )));
    }
    Ok(format!(
        "uninstalled {} on {} (trashed {live}; lineage {generation})",
        recipe.slug, host.name
    ))
}
