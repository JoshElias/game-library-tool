use crate::art::bundled_script;
use crate::inventory::GamingHost;
use crate::registry::Recipe;
use crate::status::{run_args, Remote, StatusRecord};

fn shared_gog() -> String {
    crate::config::resolved().shared_gog_root
}

fn lineage_remote() -> String {
    crate::config::resolved().lineage_remote
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
}

pub fn preview_argv(name: &str) -> Vec<String> {
    vec![
        "ludusavi".into(),
        "--no-manifest-update".into(),
        "backup".into(),
        "--preview".into(),
        name.into(),
    ]
}

pub fn canary_argv(state_home: &str, name: &str) -> Vec<String> {
    vec![
        "python3".into(),
        "-c".into(),
        format!(
            "from pathlib import Path; import hashlib,sys; key=hashlib.sha256({name:?}.encode()).hexdigest(); p=Path({state_home:?})/'game-library'/'save-lineage'/key/'canary.json'; sys.exit(0 if p.is_file() else 1)"
        ),
    ]
}

pub fn game_import_journal_argv(state_home: &str, name: &str) -> Vec<String> {
    vec![
        "python3".into(),
        "-c".into(),
        format!(
            "from pathlib import Path; import hashlib,sys; key=hashlib.sha256({name:?}.encode()).hexdigest(); root=Path({state_home:?})/'game-library'/'save-lineage'/key; sys.exit(0 if (root/'import-restore.json').is_file() and (root/'import-rollback').exists() else 1)"
        ),
    ]
}

pub fn canary_path_argv(state_home: &str, name: &str) -> Vec<String> {
    vec![
        "python3".into(),
        "-c".into(),
        format!(
            "from pathlib import Path; import hashlib; key=hashlib.sha256({name:?}.encode()).hexdigest(); print(Path({state_home:?})/'game-library'/'save-lineage'/key/'canary.json')"
        ),
    ]
}

pub fn enroll_and_wrap(
    host: &GamingHost,
    recipe: &Recipe,
    status: &StatusRecord,
    remote: &dyn Remote,
    import_root: Option<&str>,
) -> Result<String, Error> {
    if recipe.ludusavi.name.trim().is_empty() {
        return Err(Error::Message(format!(
            "{} is missing ludusavi.name",
            recipe.slug
        )));
    }
    if !status.wrap_present {
        return Err(Error::Message(format!(
            "ludusavi-lutris-wrap is missing on {}",
            host.name
        )));
    }
    if status.import_journal_blocking
        || run_args(
            remote,
            &game_import_journal_argv(&host.xdg_state_home, &recipe.ludusavi.name),
        )
        .0 == 0
    {
        return Err(Error::Message(format!(
            "{} has a blocking import journal on {}",
            recipe.slug, host.name
        )));
    }
    let (code, preview, stderr) = run_args(remote, &preview_argv(&recipe.ludusavi.name));
    if code != 0 {
        return Err(Error::Message(format!(
            "ludusavi preview failed on {}: {stderr}",
            host.name
        )));
    }
    if preview.contains("No info for these games") || !preview.contains("Size:") {
        return Err(Error::Message(format!(
            "ludusavi found no saves for {} on {}",
            recipe.ludusavi.name, host.name
        )));
    }
    if preview.contains(&host.gog_library_root)
        && !shared_gog().is_empty()
        && !preview.contains(&format!("Redirecting to: {}", shared_gog()))
    {
        return Err(Error::Message(format!(
            "{} preview touches {} without the shared GOG redirect",
            recipe.slug, host.gog_library_root
        )));
    }

    let helper = format!(
        "/home/{}/.local/bin/ludusavi-lutris-wrap",
        host.desktop_user
    );
    let mut parts = Vec::new();
    if !status.enrolled {
        parts.push(enroll(host, recipe, remote, &helper, import_root)?);
    }
    parts.push(apply_prefix(host, recipe, status, remote)?);
    Ok(format!(
        "wrapped {}: {} on {}",
        recipe.slug,
        parts.join(", "),
        host.name
    ))
}

pub fn prove_canary_argv(helper: &str, name: &str) -> Vec<String> {
    vec![
        helper.into(),
        "canary".into(),
        "--name".into(),
        name.into(),
        "--ludusavi".into(),
        "/usr/bin/ludusavi".into(),
        "--rclone".into(),
        "/usr/bin/rclone".into(),
        "--remote".into(),
        lineage_remote(),
    ]
}

pub fn harvest_root_argv(name: &str) -> Vec<String> {
    vec![
        "python3".into(),
        "/tmp/harvest_lineage_root.py".into(),
        "--name".into(),
        name.into(),
        "--rclone".into(),
        "/usr/bin/rclone".into(),
        "--remote".into(),
        lineage_remote(),
    ]
}

fn enroll(
    host: &GamingHost,
    recipe: &Recipe,
    remote: &dyn Remote,
    helper: &str,
    import_root: Option<&str>,
) -> Result<String, Error> {
    let remote_name = lineage_remote();
    if run_args(
        remote,
        &canary_argv(&host.xdg_state_home, &recipe.ludusavi.name),
    )
    .0 != 0
    {
        let (code, stdout, stderr) = remote.run(&[
            helper,
            "canary",
            "--name",
            &recipe.ludusavi.name,
            "--ludusavi",
            "/usr/bin/ludusavi",
            "--rclone",
            "/usr/bin/rclone",
            "--remote",
            &remote_name,
        ]);
        if code != 0 || !stdout.contains("canary-proven") {
            return Err(Error::Message(format!(
                "wrap canary failed on {}: {stdout}{stderr}",
                host.name
            )));
        }
    }
    let (code, stdout, stderr) = run_args(
        remote,
        &canary_path_argv(&host.xdg_state_home, &recipe.ludusavi.name),
    );
    if code != 0 {
        return Err(Error::Message(format!(
            "could not resolve canary path on {}: {stderr}",
            host.name
        )));
    }
    let canary = stdout.lines().last().unwrap_or("").trim().to_string();
    let selected = match import_root {
        Some(root) => Some(validate_uuid(root)?.to_string()),
        None => None,
    };
    if let Some(root) = selected.as_deref() {
        return enroll_import(host, recipe, remote, helper, &canary, root);
    }
    let (code, stdout, stderr) = remote.run(&[
        helper,
        "enroll",
        "--name",
        &recipe.ludusavi.name,
        "--ludusavi",
        "/usr/bin/ludusavi",
        "--rclone",
        "/usr/bin/rclone",
        "--remote",
        &remote_name,
        "--canary-evidence",
        &canary,
        "--capture-new",
    ]);
    let blob = format!("{stdout}{stderr}");
    if blob.contains("remote lineage exists") {
        let harvested = harvest_unique_root(host, recipe, remote)?;
        return enroll_import(host, recipe, remote, helper, &canary, &harvested);
    }
    if code != 0 && !blob.contains("already enrolled") && !stdout.contains("enrolled") {
        return Err(Error::Message(format!(
            "enroll failed on {}: {blob}",
            host.name
        )));
    }
    Ok("enrolled".into())
}

fn validate_uuid(value: &str) -> Result<&str, Error> {
    let bytes = value.as_bytes();
    let ok = bytes.len() == 36
        && [8usize, 13, 18, 23]
            .iter()
            .all(|&index| bytes[index] == b'-')
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 8 | 13 | 18 | 23) || byte.is_ascii_hexdigit());
    if ok {
        Ok(value)
    } else {
        Err(Error::Message(format!("invalid import-root uuid: {value}")))
    }
}

fn harvest_unique_root(
    host: &GamingHost,
    recipe: &Recipe,
    remote: &dyn Remote,
) -> Result<String, Error> {
    let bytes = std::fs::read(bundled_script("harvest_lineage_root.py"))
        .map_err(|error| Error::Message(error.to_string()))?;
    remote
        .write_file("/tmp/harvest_lineage_root.py", &bytes)
        .map_err(Error::Message)?;
    let (code, stdout, stderr) = run_args(remote, &harvest_root_argv(&recipe.ludusavi.name));
    let line = stdout.lines().last().unwrap_or("").trim();
    if let Some(root) = line.strip_prefix("unique ") {
        return validate_uuid(root).map(ToOwned::to_owned);
    }
    if line.starts_with("competing") {
        return Err(Error::Message(format!(
            "{} has competing remote roots on {}; refuse to pick one",
            recipe.slug, host.name
        )));
    }
    Err(Error::Message(format!(
        "could not harvest a unique import-root on {}: {line} {stderr} ({code})",
        host.name
    )))
}

fn enroll_import(
    host: &GamingHost,
    recipe: &Recipe,
    remote: &dyn Remote,
    helper: &str,
    canary: &str,
    root: &str,
) -> Result<String, Error> {
    let remote_name = lineage_remote();
    let (code, stdout, stderr) = remote.run(&[
        helper,
        "enroll",
        "--name",
        &recipe.ludusavi.name,
        "--ludusavi",
        "/usr/bin/ludusavi",
        "--rclone",
        "/usr/bin/rclone",
        "--remote",
        &remote_name,
        "--canary-evidence",
        canary,
        "--import-root",
        root,
    ]);
    let blob = format!("{stdout}{stderr}");
    if code != 0 && !stdout.contains("enrolled") {
        return Err(Error::Message(format!(
            "import-root {root} failed on {}: {blob}",
            host.name
        )));
    }
    rewrite_nwn_home_aliases(host, recipe, remote)?;
    Ok(format!("imported {root}"))
}

fn rewrite_nwn_home_aliases(
    host: &GamingHost,
    recipe: &Recipe,
    remote: &dyn Remote,
) -> Result<(), Error> {
    if !recipe
        .ludusavi
        .name
        .to_ascii_lowercase()
        .contains("neverwinter nights")
    {
        return Ok(());
    }
    let bytes = std::fs::read(bundled_script("rewrite_nwn_home_aliases.py"))
        .map_err(|error| Error::Message(error.to_string()))?;
    remote
        .write_file("/tmp/rewrite_nwn_home_aliases.py", &bytes)
        .map_err(Error::Message)?;
    let home = format!("/home/{}", host.desktop_user);
    let (code, stdout, stderr) = remote.run(&[
        "python3",
        "/tmp/rewrite_nwn_home_aliases.py",
        "--home",
        &home,
    ]);
    if code != 0 {
        return Err(Error::Message(format!(
            "NWN home-alias rewrite failed on {}: {stdout}{stderr}",
            host.name
        )));
    }
    Ok(())
}

fn apply_steam_wrap(
    host: &GamingHost,
    recipe: &Recipe,
    remote: &dyn Remote,
) -> Result<String, Error> {
    if remote.run(&["pgrep", "-x", "steam"]).0 == 0
        || remote.run(&["pgrep", "-f", "steamwebhelper"]).0 == 0
    {
        return Err(Error::Message(
            "Steam is running; close it before applying launch options".into(),
        ));
    }
    let bytes = std::fs::read(bundled_script("set_steam_wrap.py"))
        .map_err(|error| Error::Message(error.to_string()))?;
    remote
        .write_file("/tmp/set_steam_wrap.py", &bytes)
        .map_err(Error::Message)?;
    let (code, stdout, stderr) = remote.run(&[
        "python3",
        "/tmp/set_steam_wrap.py",
        "--appid",
        &recipe.product_id,
        "--user",
        &host.desktop_user,
        "--name",
        &recipe.ludusavi.name,
    ]);
    if code != 0 {
        return Err(Error::Message(format!(
            "steam wrap apply failed on {}: {stdout}{stderr}",
            host.name
        )));
    }
    Ok(if stdout.contains("already") {
        "steam wrap already set".into()
    } else {
        "steam wrap applied".into()
    })
}

fn apply_prefix(
    host: &GamingHost,
    recipe: &Recipe,
    status: &StatusRecord,
    remote: &dyn Remote,
) -> Result<String, Error> {
    if matches!(recipe.source, crate::registry::Source::Steam) {
        return apply_steam_wrap(host, recipe, remote);
    }
    let lutris_id = status.lutris_id.ok_or_else(|| {
        Error::Message(format!("{} has no Lutris id on {}", recipe.slug, host.name))
    })?;
    if remote.run(&["pgrep", "-x", "lutris"]).0 == 0 {
        return Err(Error::Message(
            "Lutris is running; close it before applying the wrap prefix".into(),
        ));
    }
    let bytes = std::fs::read(bundled_script("set_lutris_prefix.py"))
        .map_err(|error| Error::Message(error.to_string()))?;
    remote
        .write_file("/tmp/set_lutris_prefix.py", &bytes)
        .map_err(Error::Message)?;
    let prefix = format!(
        "/home/{}/.local/bin/ludusavi-lutris-wrap run --name {} --ludusavi /usr/bin/ludusavi --rclone /usr/bin/rclone --",
        host.desktop_user,
        shell_single_quote(&recipe.ludusavi.name)
    );
    let backup = format!(
        "{}/game-library/lutris-prefix-rollback/{}-{}.pre-helper.yml",
        host.xdg_state_home, recipe.slug, lutris_id
    );
    let id = lutris_id.to_string();
    let (code, stdout, stderr) = remote.run(&[
        "python3",
        "/tmp/set_lutris_prefix.py",
        "--lutris-id",
        &id,
        "--prefix",
        &prefix,
        "--backup",
        &backup,
    ]);
    if code == 2 || stdout.trim() == "other" {
        return Err(Error::Message(format!(
            "{} already has a different Lutris prefix on {}",
            recipe.slug, host.name
        )));
    }
    if code != 0 {
        return Err(Error::Message(format!(
            "prefix apply failed on {}: {stdout}{stderr}",
            host.name
        )));
    }
    Ok(if stdout.contains("already") {
        "prefix already set".into()
    } else {
        "prefix applied".into()
    })
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
