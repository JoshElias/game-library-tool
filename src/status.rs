use minijinja::Environment;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::inventory::GamingHost;
use crate::legacy::is_legacy_path;
use crate::registry::{Recipe, Source};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatusRecord {
    pub host: String,
    pub alias: String,
    pub desktop_user: String,
    pub recipe_installable: bool,
    pub canonical_directory: String,
    pub live_directory: Option<String>,
    pub installed: bool,
    pub lutris_id: Option<i64>,
    pub lutris_slug: Option<String>,
    pub runner: Option<String>,
    pub wrap_present: bool,
    pub enrolled: bool,
    pub generation: Option<String>,
    pub import_journal_blocking: bool,
    pub graphical_session: bool,
    pub lutris_version: String,
    pub ludusavi_version: String,
    pub legacy: bool,
    pub art_applied: bool,
    pub complete: bool,
    pub notes: Vec<String>,
}

pub trait Remote {
    fn run(&self, argv: &[&str]) -> (i32, String, String);
    fn write_file(&self, _dest: &str, _contents: &[u8]) -> Result<(), String> {
        Err("remote file write is not supported".into())
    }
}

pub fn canonical_directory(host: &GamingHost, recipe: &Recipe) -> Result<String, String> {
    if recipe.lutris.directory_template.is_empty() {
        return Ok(host.rendered_gog_directory(&recipe.slug));
    }
    let env = Environment::new();
    let mut ctx = serde_json::Map::new();
    ctx.insert(
        "gaming_workstations_gog_library_root".into(),
        Value::String(host.gog_library_root.clone()),
    );
    ctx.insert(
        "gaming_workstations_game_library_root".into(),
        Value::String(host.game_library_root.clone()),
    );
    ctx.insert(
        "gaming_workstations_xdg_data_home".into(),
        Value::String(host.xdg_data_home.clone()),
    );
    ctx.insert(
        "workstation_user".into(),
        Value::String(host.desktop_user.clone()),
    );
    ctx.insert(
        "workstation_user_home".into(),
        Value::String(format!("/home/{}", host.desktop_user)),
    );
    env.render_str(&recipe.lutris.directory_template, Value::Object(ctx))
        .map_err(|error| error.to_string())
}

pub fn collect_status(host: &GamingHost, recipe: &Recipe, remote: &dyn Remote) -> StatusRecord {
    let inventory_root = host.installation_root(&recipe.slug);
    let (live_directory, lutris_id, lutris_slug, runner) = if matches!(recipe.source, Source::Steam)
    {
        steam_identity(remote, host, recipe)
    } else {
        installed_identity(remote, recipe)
    };
    let target = if matches!(recipe.source, Source::Steam) {
        live_directory
            .clone()
            .unwrap_or_else(|| format!("{}/Steam/steamapps/common", host.xdg_data_home))
    } else {
        canonical_directory(host, recipe)
            .unwrap_or_else(|_| host.rendered_gog_directory(&recipe.slug))
    };
    let lutris_version = first_line(remote.run(&["lutris", "--version"]));
    let ludusavi_version = first_line(remote.run(&["ludusavi", "--version"]));
    let wrap = format!(
        "/home/{}/.local/bin/ludusavi-lutris-wrap",
        host.desktop_user
    );
    let wrap_present = remote.run(&["test", "-x", &wrap]).0 == 0;
    let graphical = has_graphical_session(remote);
    let (enrolled, generation) = lineage_status(remote, &host.desktop_user, &recipe.ludusavi.name);
    let journal = format!(
        "{}/game-library/save-lineage/import-restore.json",
        host.xdg_state_home
    );
    let import_journal = remote.run(&["test", "-e", &journal]).0 == 0;
    let live_legacy = is_legacy_path(live_directory.as_deref(), Some(&target));
    let inventory_legacy =
        is_legacy_path(inventory_root, Some(&target)) && live_directory.is_none();
    let legacy = live_legacy || inventory_legacy;
    let mut notes = Vec::new();
    if legacy {
        notes.push("legacy; skipped".to_string());
    }
    let installed = if matches!(recipe.source, Source::Steam) {
        live_directory.is_some() && !legacy
    } else {
        live_directory
            .as_deref()
            .is_some_and(|path| !legacy && path == target)
    };
    let art_applied = lutris_id
        .is_some_and(|id| run_args(remote, &art_journal_argv(&host.desktop_user, id)).0 == 0);
    if recipe.steam_shortcut && installed && !art_applied {
        notes.push("steam art not applied".to_string());
    }
    let steam_wrapped =
        !matches!(recipe.source, Source::Steam) || steam_wrap_applied(remote, &recipe.product_id);
    if matches!(recipe.source, Source::Steam) && installed && !steam_wrapped {
        notes.push("steam wrap not applied".to_string());
    }
    let complete = installed
        && wrap_present
        && enrolled
        && steam_wrapped
        && !import_journal
        && !legacy
        && recipe.installable
        && (!recipe.steam_shortcut || art_applied);
    StatusRecord {
        host: host.name.clone(),
        alias: host.alias.clone(),
        desktop_user: host.desktop_user.clone(),
        recipe_installable: recipe.installable,
        canonical_directory: target,
        live_directory,
        installed,
        lutris_id,
        lutris_slug,
        runner,
        wrap_present,
        enrolled,
        generation,
        import_journal_blocking: import_journal,
        graphical_session: graphical,
        lutris_version,
        ludusavi_version,
        legacy,
        art_applied,
        complete,
        notes,
    }
}

fn first_line(result: (i32, String, String)) -> String {
    if result.0 != 0 {
        return String::new();
    }
    result.1.lines().next().unwrap_or("").trim().to_string()
}

pub fn has_graphical_session(remote: &dyn Remote) -> bool {
    let (code, stdout, _) = remote.run(&["loginctl", "list-sessions", "--no-legend"]);
    if code != 0 {
        return false;
    }
    let lowered = stdout.to_ascii_lowercase();
    if lowered.contains("wayl") || lowered.contains("x11") || lowered.contains("graphical") {
        return true;
    }
    for id in stdout.split_whitespace() {
        if !id.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        let (code, detail, _) =
            remote.run(&["loginctl", "show-session", id, "-p", "Type", "-p", "Class"]);
        if code != 0 {
            continue;
        }
        let detail = detail.to_ascii_lowercase();
        if (detail.contains("type=wayland") || detail.contains("type=x11"))
            && !detail.contains("class=manager")
        {
            return true;
        }
    }
    false
}

fn lineage_status(remote: &dyn Remote, desktop_user: &str, name: &str) -> (bool, Option<String>) {
    if name.is_empty() {
        return (false, None);
    }
    let helper = format!("/home/{desktop_user}/.local/bin/ludusavi-lutris-wrap");
    let (code, stdout, _) = remote.run(&[&helper, "status", "--name", name]);
    if code != 0 || stdout.trim().is_empty() {
        return (false, None);
    }
    let Ok(payload) = serde_json::from_str::<Value>(&stdout) else {
        return (false, None);
    };
    let enrolled = payload
        .get("enrolled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let generation = payload
        .get("generation")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    (enrolled, generation)
}

pub fn extract_json_array(stdout: &str) -> Option<&str> {
    let start = stdout.find('[')?;
    let end = stdout.rfind(']')?;
    Some(&stdout[start..=end])
}

fn installed_identity(
    remote: &dyn Remote,
    recipe: &Recipe,
) -> (Option<String>, Option<i64>, Option<String>, Option<String>) {
    let (code, stdout, _) = remote.run(&["lutris", "--list-games", "--installed", "--json"]);
    if code != 0 || stdout.trim().is_empty() {
        return (None, None, None, None);
    }
    let Ok(games) = serde_json::from_str::<Value>(extract_json_array(&stdout).unwrap_or(&stdout))
    else {
        return (None, None, None, None);
    };
    let Some(games) = games.as_array() else {
        return (None, None, None, None);
    };
    for game in games {
        let slug = game.get("slug").and_then(Value::as_str).unwrap_or("");
        let name = game.get("name").and_then(Value::as_str).unwrap_or("");
        if slug == recipe.slug || name == recipe.title {
            let directory = game
                .get("directory")
                .or_else(|| game.get("directory_path"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let lutris_id = game.get("id").and_then(Value::as_i64);
            let runner = game
                .get("runner")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            return (
                directory,
                lutris_id,
                if slug.is_empty() {
                    None
                } else {
                    Some(slug.to_string())
                },
                runner,
            );
        }
    }
    (None, None, None, None)
}

fn steam_identity(
    remote: &dyn Remote,
    _host: &GamingHost,
    recipe: &Recipe,
) -> (Option<String>, Option<i64>, Option<String>, Option<String>) {
    if !recipe.product_id.chars().all(|ch| ch.is_ascii_digit()) || recipe.product_id.is_empty() {
        return (None, None, None, None);
    }
    let script = concat!(
        "from pathlib import Path; import json,re,sys;",
        "app=sys.argv[1]; home=Path.home(); roots=[];",
        "lf=home/'.local/share/Steam/steamapps/libraryfolders.vdf';",
        "roots=[] if not lf.is_file() else [Path(m.group(1)) for m in re.finditer(r'\"path\"\\s+\"([^\"]+)\"', lf.read_text(errors='replace'))];",
        "[roots.append(p) for p in (home/'.local/share/Steam', home/'.local/share/games/steam') if p.is_dir() and p not in roots];",
        "out={'installed':False,'directory':''};",
        "\nfor root in roots:\n",
        " acf=root/'steamapps'/('appmanifest_%s.acf'%app)\n",
        " if acf.is_file():\n",
        "  vals=dict(re.findall(r'\"([^\"]+)\"\\s+\"([^\"]*)\"', acf.read_text(errors='replace')))\n",
        "  dest=str(root/'steamapps'/'common'/vals.get('installdir',''))\n",
        "  flags=int(vals.get('StateFlags') or '0')\n",
        "  out={'installed': bool(flags&4) and Path(dest).is_dir(),'directory':dest}\n",
        "  break\n",
        "print(json.dumps(out))"
    );
    let (code, stdout, _) = remote.run(&["python3", "-c", script, &recipe.product_id]);
    if code != 0 {
        return (None, None, None, Some("steam".into()));
    }
    let Ok(payload) = serde_json::from_str::<Value>(stdout.trim()) else {
        return (None, None, None, Some("steam".into()));
    };
    let directory = payload
        .get("directory")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let installed = payload
        .get("installed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    (
        if installed { directory } else { None },
        None,
        None,
        Some("steam".into()),
    )
}

fn steam_wrap_applied(remote: &dyn Remote, appid: &str) -> bool {
    if !appid.chars().all(|ch| ch.is_ascii_digit()) || appid.is_empty() {
        return false;
    }
    let script = concat!(
        "from pathlib import Path; import sys;",
        "app=sys.argv[1]; root=Path.home()/'.local/share/Steam/userdata';",
        "print('yes' if any(app in (t:=p.read_text(errors='replace')) and 'ludusavi-lutris-wrap' in t and '%command%' in t for p in root.glob('*/config/localconfig.vdf')) else 'no')"
    );
    let (code, stdout, _) = remote.run(&["python3", "-c", script, appid]);
    code == 0 && stdout.trim() == "yes"
}

pub fn run_args(remote: &dyn Remote, argv: &[String]) -> (i32, String, String) {
    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    remote.run(&refs)
}

pub fn art_journal_argv(desktop_user: &str, lutris_id: i64) -> Vec<String> {
    vec![
        "python3".into(),
        "-c".into(),
        format!(
            "from pathlib import Path; import sys; root=Path('/home/{desktop_user}/.local/state/game-library-artwork'); sys.exit(0 if root.is_dir() and any(p.name.startswith('{lutris_id}-') and (p/'manifest.json').is_file() for p in root.iterdir()) else 1)"
        ),
    ]
}

pub fn shortcuts_vdf_argv(desktop_user: &str) -> Vec<String> {
    vec![
        "python3".into(),
        "-c".into(),
        format!(
            "from pathlib import Path; home=Path('/home/{desktop_user}'); roots=[home/'.local/share/Steam/userdata', home/'.steam/steam/userdata']; found=sorted({{str(p.resolve()) for r in roots if r.is_dir() for p in r.glob('*/config/shortcuts.vdf')}}); print(found[0] if found else '')"
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MapRemote(HashMap<Vec<String>, (i32, String, String)>);

    impl Remote for MapRemote {
        fn run(&self, argv: &[&str]) -> (i32, String, String) {
            let key: Vec<String> = argv.iter().map(|part| (*part).to_string()).collect();
            self.0
                .get(&key)
                .cloned()
                .unwrap_or((1, String::new(), "missing".into()))
        }
    }

    #[test]
    fn steamdeck_kde_session_counts_as_graphical() {
        let mut map = HashMap::new();
        map.insert(
            vec![
                "loginctl".into(),
                "list-sessions".into(),
                "--no-legend".into(),
            ],
            (
                0,
                "111 1000 deck - 21147 user - no -\n  3 1000 deck seat0 3960 user tty1 no -\n"
                    .into(),
                String::new(),
            ),
        );
        map.insert(
            vec![
                "loginctl".into(),
                "show-session".into(),
                "3".into(),
                "-p".into(),
                "Type".into(),
                "-p".into(),
                "Class".into(),
            ],
            (0, "Type=wayland\nClass=user\n".into(), String::new()),
        );
        assert!(has_graphical_session(&MapRemote(map)));
    }
}
