use crate::inventory::GamingHost;
use crate::registry::{Recipe, Source};
use crate::status::{extract_json_array, run_args, Remote, StatusRecord};

const MIN_FREE_BYTES: u64 = 15 * 1024 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
}

pub fn lutris_game_path_argv() -> Vec<String> {
    vec![
        "python3".into(),
        "-c".into(),
        "from lutris.config import LutrisConfig; print((LutrisConfig().system_config or {}).get('game_path') or '')".into(),
    ]
}

pub fn dest_busy_argv(path: &str) -> Vec<String> {
    vec![
        "python3".into(),
        "-c".into(),
        format!(
            "from pathlib import Path; import sys; p=Path({path:?}); sys.exit(1 if p.exists() and any(p.iterdir()) else 0)"
        ),
    ]
}

pub fn free_bytes_argv(path: &str) -> Vec<String> {
    vec![
        "df".into(),
        "-B1".into(),
        "--output=avail".into(),
        path.into(),
    ]
}

pub fn start_gog_download(
    host: &GamingHost,
    recipe: &Recipe,
    status: &StatusRecord,
    remote: &dyn Remote,
) -> Result<String, Error> {
    if !matches!(recipe.source, Source::Gog) {
        return Err(Error::Message(format!(
            "{} is not a GOG title; download refused",
            recipe.slug
        )));
    }
    if recipe.product_id.trim().is_empty() {
        return Err(Error::Message(format!(
            "{} is missing product_id",
            recipe.slug
        )));
    }
    if !status.graphical_session {
        return Err(Error::Message(format!(
            "{} has no graphical session; cannot start a visible Lutris installer",
            host.name
        )));
    }
    if remote.run(&["pgrep", "-x", "lutris"]).0 == 0 {
        return Err(Error::Message(
            "Lutris is running; close it and re-run install".into(),
        ));
    }
    if !gog_owned(remote, recipe) {
        return Err(Error::Message(format!(
            "{} / {} is not in the authenticated GOG library on {}",
            recipe.title, recipe.product_id, host.name
        )));
    }
    let (code, stdout, stderr) = run_args(remote, &lutris_game_path_argv());
    if code != 0 {
        return Err(Error::Message(format!(
            "could not read Lutris game_path on {}: {stderr}",
            host.name
        )));
    }
    let game_path = stdout.lines().last().unwrap_or("").trim();
    let expected = format!("{}/gog/{}", game_path, recipe.slug);
    if game_path.is_empty() || expected != status.canonical_directory {
        return Err(Error::Message(format!(
            "Lutris game_path/gog/{} is {expected:?}, need {}",
            recipe.slug, status.canonical_directory
        )));
    }
    if run_args(remote, &dest_busy_argv(&status.canonical_directory)).0 != 0 {
        return Err(Error::Message(format!(
            "destination {} is not empty",
            status.canonical_directory
        )));
    }
    let parent = status
        .canonical_directory
        .rsplit_once('/')
        .map(|(head, _)| head)
        .unwrap_or(&status.canonical_directory);
    let (code, stdout, _) = run_args(remote, &free_bytes_argv(parent));
    if code != 0 || parse_df_avail(&stdout).is_none_or(|bytes| bytes < MIN_FREE_BYTES) {
        return Err(Error::Message(format!(
            "need at least {MIN_FREE_BYTES} free bytes on {parent}"
        )));
    }
    let uri = format!("lutris:install/gog:{}", recipe.product_id.trim());
    let (code, stdout, stderr) = remote.run(&[
        "gdbus",
        "call",
        "--session",
        "--dest",
        "org.freedesktop.portal.Desktop",
        "--object-path",
        "/org/freedesktop/portal/desktop",
        "--method",
        "org.freedesktop.portal.OpenURI.OpenURI",
        "",
        &uri,
        "{}",
    ]);
    if code != 0 || !stdout.contains("objectpath") {
        return Err(Error::Message(format!(
            "desktop portal refused {uri} on {}: {stdout}{stderr}",
            host.name
        )));
    }
    Ok(format!(
        "started visible GOG installer {uri} on {}; do not cancel the desktop Lutris window",
        host.name
    ))
}

fn gog_owned(remote: &dyn Remote, recipe: &Recipe) -> bool {
    let (code, stdout, _) = remote.run(&["lutris", "--list-service-games", "gog", "--json"]);
    if code != 0 {
        return false;
    }
    let Ok(games) =
        serde_json::from_str::<serde_json::Value>(extract_json_array(&stdout).unwrap_or(&stdout))
    else {
        return false;
    };
    let Some(games) = games.as_array() else {
        return false;
    };
    games.iter().any(|game| {
        let appid = game
            .get("appid")
            .or_else(|| game.get("service_id"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let slug = game
            .get("slug")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        appid == recipe.product_id
            || slug == recipe.slug
            || slug.replace('-', "_") == recipe.slug.replace('-', "_")
    })
}

fn parse_df_avail(stdout: &str) -> Option<u64> {
    stdout
        .lines()
        .map(str::trim)
        .find(|line| line.chars().all(|ch| ch.is_ascii_digit()))
        .and_then(|line| line.parse().ok())
}
