use std::fs;

use tempfile::TempDir;

use game_library::config::with_locked_env;
use game_library::hosts_file::parse_hosts_yaml;
use game_library::inventory::{load_host, load_hosts};

const EXAMPLE: &str = r#"
hosts:
  - name: example
    user: alice
    ssh: example-games
    xdg_config_home: /home/alice/.config
    xdg_data_home: /home/alice/.local/share
    xdg_state_home: /home/alice/.local/state
    xdg_cache_home: /home/alice/.cache
    game_library_root: /home/alice/.local/share/games
    gog_library_root: /home/alice/.local/share/games/gog
"#;

#[test]
fn parses_generic_host_file() {
    let hosts = parse_hosts_yaml(EXAMPLE).unwrap();
    assert_eq!(hosts.len(), 1);
    assert_eq!(hosts[0].name, "example");
    assert_eq!(hosts[0].alias, "example-games");
    assert_eq!(hosts[0].desktop_user, "alice");
    assert_eq!(
        hosts[0].gog_library_root,
        "/home/alice/.local/share/games/gog"
    );
}

#[test]
fn rejects_relative_paths() {
    let err = parse_hosts_yaml(
        r#"
hosts:
  - name: example
    user: alice
    xdg_config_home: .config
    xdg_data_home: /home/alice/.local/share
    xdg_state_home: /home/alice/.local/state
    xdg_cache_home: /home/alice/.cache
    game_library_root: /home/alice/.local/share/games
    gog_library_root: /home/alice/.local/share/games/gog
"#,
    )
    .unwrap_err();
    assert!(err.to_string().contains("absolute"));
}

#[test]
fn hosts_env_file_wins_over_ansible() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("hosts.yaml");
    fs::write(&path, EXAMPLE).unwrap();
    with_locked_env(
        &[
            ("GAME_LIBRARY_HOSTS", Some(path.to_str().unwrap())),
            ("GAME_LIBRARY_CONFIG", None),
        ],
        || {
            let hosts = load_hosts().unwrap();
            assert_eq!(hosts[0].name, "example");
            assert!(load_host("daemon").is_err());
            assert_eq!(load_host("example").unwrap().alias, "example-games");
        },
    );
}
