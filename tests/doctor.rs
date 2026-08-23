use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;

use game_library::config::{with_locked_env, DEFAULT_LINEAGE_REMOTE, DEFAULT_SHARED_GOG};
use game_library::doctor::inspect;
use game_library::inventory::GamingHost;
use game_library::status::Remote;

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

fn host() -> GamingHost {
    GamingHost {
        name: "example".into(),
        alias: "example-games".into(),
        desktop_user: "alice".into(),
        xdg_config_home: "/home/alice/.config".into(),
        xdg_data_home: "/home/alice/.local/share".into(),
        xdg_state_home: "/home/alice/.local/state".into(),
        xdg_cache_home: "/home/alice/.cache".into(),
        game_library_root: "/home/alice/.local/share/games".into(),
        gog_library_root: "/home/alice/.local/share/games/gog".into(),
        gog_installation_roots: std::collections::BTreeMap::new(),
    }
}

#[test]
fn healthy_host_is_reachable() {
    let mut map = HashMap::new();
    map.insert(
        vec!["id".into(), "-un".into()],
        (0, "alice\n".into(), String::new()),
    );
    map.insert(
        vec!["lutris".into(), "--version".into()],
        (0, "lutris-0.5.22\n".into(), String::new()),
    );
    map.insert(
        vec!["ludusavi".into(), "--version".into()],
        (0, "ludusavi 0.31.0\n".into(), String::new()),
    );
    map.insert(
        vec![
            "test".into(),
            "-x".into(),
            "/home/alice/.local/bin/ludusavi-lutris-wrap".into(),
        ],
        (0, String::new(), String::new()),
    );
    map.insert(
        vec![
            "test".into(),
            "-d".into(),
            "/home/alice/.local/share".into(),
        ],
        (0, String::new(), String::new()),
    );
    map.insert(
        vec![
            "test".into(),
            "-d".into(),
            "/home/alice/.local/share/games/gog".into(),
        ],
        (0, String::new(), String::new()),
    );
    map.insert(
        vec![
            "loginctl".into(),
            "list-sessions".into(),
            "--no-legend".into(),
        ],
        (0, "1 alice wayland active\n".into(), String::new()),
    );
    let report = inspect(&host(), &MapRemote(map));
    assert!(report.reachable);
    assert!(report.identity_ok);
    assert!(report.graphical_session);
    assert!(report.wrap_present);
    assert!(report.xdg_ok);
    assert!(report.notes.is_empty());
}

#[test]
fn dead_probe_is_not_proof_of_missing_games() {
    let report = inspect(&host(), &MapRemote(HashMap::new()));
    assert!(!report.reachable);
    assert!(report
        .notes
        .iter()
        .any(|note| note.contains("do not treat games as missing")));
}

#[test]
fn config_env_overrides_private_defaults() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.yaml");
    fs::write(
        &path,
        "ssh_helper: /tmp/custom-ssh\nlineage_remote: my-remote\nshared_gog_root: /tmp/shared/gog\n",
    )
    .unwrap();
    with_locked_env(
        &[
            ("GAME_LIBRARY_CONFIG", Some(path.to_str().unwrap())),
            ("GAME_LIBRARY_SSH", None),
            ("GAME_LIBRARY_LINEAGE_REMOTE", None),
            ("GAME_LIBRARY_SHARED_GOG", None),
            ("GAME_LIBRARY_HOSTS", None),
        ],
        || {
            let resolved = game_library::config::resolved();
            assert_eq!(resolved.ssh_helper, PathBuf::from("/tmp/custom-ssh"));
            assert_eq!(resolved.lineage_remote, "my-remote");
            assert_eq!(resolved.shared_gog_root, "/tmp/shared/gog/");
        },
    );
}

#[test]
fn defaults_are_public() {
    with_locked_env(
        &[
            (
                "GAME_LIBRARY_CONFIG",
                Some("/tmp/missing-game-library-config.yaml"),
            ),
            ("GAME_LIBRARY_SSH", None),
            ("GAME_LIBRARY_LINEAGE_REMOTE", None),
            ("GAME_LIBRARY_SHARED_GOG", None),
            ("GAME_LIBRARY_HOSTS", None),
        ],
        || {
            let resolved = game_library::config::resolved();
            assert_eq!(
                resolved.ssh_helper,
                PathBuf::from(game_library::ssh::DEFAULT_SSH)
            );
            assert_eq!(resolved.lineage_remote, DEFAULT_LINEAGE_REMOTE);
            assert_eq!(resolved.shared_gog_root, DEFAULT_SHARED_GOG);
            assert!(resolved.hosts_file.is_none());
        },
    );
}
