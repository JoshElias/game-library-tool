use std::path::{Path, PathBuf};

use crate::inventory::GamingHost;
use crate::registry::Recipe;
use crate::status::{run_args, shortcuts_vdf_argv, Remote, StatusRecord};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
}

pub fn bundled_script(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join(name)
}

pub fn apply_pinned_art(
    host: &GamingHost,
    recipe: &Recipe,
    status: &StatusRecord,
    remote: &dyn Remote,
) -> Result<String, Error> {
    if !recipe.steam_shortcut {
        return Ok("steam shortcut disabled".into());
    }
    if !recipe.steamgriddb.complete() {
        return Err(Error::Message(
            "steamgriddb IDs are required when steam_shortcut is true".into(),
        ));
    }
    let lutris_id = status.lutris_id.ok_or_else(|| {
        Error::Message(format!(
            "{} is not installed on {}; cannot create a Steam shortcut",
            recipe.slug, host.name
        ))
    })?;
    if remote.run(&["pgrep", "-x", "lutris"]).0 == 0 {
        return Err(Error::Message(
            "Lutris is running; close it and re-run install".into(),
        ));
    }
    let key = format!(
        "/home/{}/.config/steam-shortcut-artwork/steamgriddb-api-key",
        host.desktop_user
    );
    if remote.run(&["test", "-f", &key]).0 != 0 {
        return Err(Error::Message(format!(
            "SteamGridDB key missing on {} at ~/.config/steam-shortcut-artwork/steamgriddb-api-key",
            host.name
        )));
    }
    let artwork = bundled_script("steam_shortcut_artwork.py");
    let create = bundled_script("create_lutris_steam_shortcut.py");
    push_script(remote, &artwork, "/tmp/steam_shortcut_artwork.py")?;
    push_script(remote, &create, "/tmp/create_lutris_steam_shortcut.py")?;

    let (code, stdout, stderr) = run_args(remote, &shortcuts_vdf_argv(&host.desktop_user));
    if code != 0 {
        return Err(Error::Message(format!(
            "could not locate shortcuts.vdf on {}: {stderr}",
            host.name
        )));
    }
    let shortcuts = stdout.lines().next().unwrap_or("").trim().to_string();
    if shortcuts.is_empty() {
        return Err(Error::Message(format!(
            "no Steam shortcuts.vdf on {}",
            host.name
        )));
    }

    let id = lutris_id.to_string();
    let exists = remote
        .run(&[
            "python3",
            "/tmp/create_lutris_steam_shortcut.py",
            "--lutris-id",
            &id,
            "--exists",
        ])
        .0
        == 0;
    if !exists {
        let baseline = format!("/run/user/1000/game-library-art-baseline-{lutris_id}.json");
        let _ = remote.run(["rm", "-f", &baseline].as_slice());
        let (code, stdout, stderr) = remote.run(&[
            "python3",
            "/tmp/steam_shortcut_artwork.py",
            "--capture-steam-baseline",
            "--shortcuts",
            &shortcuts,
            "--baseline",
            &baseline,
        ]);
        if code != 0 {
            return Err(Error::Message(format!(
                "baseline failed on {}: {stdout}{stderr}",
                host.name
            )));
        }
        let (code, stdout, stderr) = remote.run(&[
            "python3",
            "/tmp/create_lutris_steam_shortcut.py",
            "--lutris-id",
            &id,
        ]);
        if code != 0 {
            return Err(Error::Message(format!(
                "shortcut create failed on {}: {stdout}{stderr}",
                host.name
            )));
        }
        let pins = recipe.steamgriddb.clone();
        let game_id = pins.game_id.to_string();
        let landscape = pins.landscape_id.to_string();
        let capsule = pins.capsule_id.to_string();
        let hero = pins.hero_id.to_string();
        let logo = pins.logo_id.to_string();
        let icon = pins.icon_id.to_string();
        let (code, stdout, stderr) = remote.run(&[
            "python3",
            "/tmp/steam_shortcut_artwork.py",
            "--lutris-id",
            &id,
            "--shortcuts",
            &shortcuts,
            "--baseline",
            &baseline,
            "--sgdb-game-id",
            &game_id,
            "--landscape-art-id",
            &landscape,
            "--capsule-art-id",
            &capsule,
            "--hero-art-id",
            &hero,
            "--logo-art-id",
            &logo,
            "--icon-art-id",
            &icon,
        ]);
        if code != 0 {
            return Err(Error::Message(format!(
                "art apply failed on {}: {stdout}{stderr}",
                host.name
            )));
        }
        if stdout.contains("SKIP") {
            return Ok(format!(
                "skipped existing custom art: {} on {}",
                recipe.slug, host.name
            ));
        }
        return Ok(format!(
            "applied pinned SteamGridDB art: {} on {}",
            recipe.slug, host.name
        ));
    }

    let pins = recipe.steamgriddb.clone();
    let game_id = pins.game_id.to_string();
    let landscape = pins.landscape_id.to_string();
    let capsule = pins.capsule_id.to_string();
    let hero = pins.hero_id.to_string();
    let logo = pins.logo_id.to_string();
    let icon = pins.icon_id.to_string();
    let preview = remote.run(&[
        "python3",
        "/tmp/steam_shortcut_artwork.py",
        "--lutris-id",
        &id,
        "--shortcuts",
        &shortcuts,
        "--preview",
    ]);
    if preview.1.contains("skip-existing-custom-art") || preview.1.contains("SKIP") {
        return Ok(format!(
            "skipped existing custom art: {} on {}",
            recipe.slug, host.name
        ));
    }
    let (code, stdout, stderr) = remote.run(&[
        "python3",
        "/tmp/steam_shortcut_artwork.py",
        "--lutris-id",
        &id,
        "--shortcuts",
        &shortcuts,
        "--sgdb-game-id",
        &game_id,
        "--landscape-art-id",
        &landscape,
        "--capsule-art-id",
        &capsule,
        "--hero-art-id",
        &hero,
        "--logo-art-id",
        &logo,
        "--icon-art-id",
        &icon,
    ]);
    if code != 0 {
        return Err(Error::Message(format!(
            "art apply failed on {}: {stdout}{stderr}",
            host.name
        )));
    }
    if stdout.contains("SKIP") {
        return Ok(format!(
            "skipped existing custom art: {} on {}",
            recipe.slug, host.name
        ));
    }
    Ok(format!(
        "applied pinned SteamGridDB art: {} on {}",
        recipe.slug, host.name
    ))
}

fn push_script(remote: &dyn Remote, local: &Path, dest: &str) -> Result<(), Error> {
    let bytes = std::fs::read(local).map_err(|error| Error::Message(error.to_string()))?;
    remote.write_file(dest, &bytes).map_err(Error::Message)
}
