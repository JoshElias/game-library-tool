use std::collections::HashMap;

use game_library::inventory::GamingHost;
use game_library::library::collect_library;
use game_library::registry::parse_recipe;
use game_library::status::Remote;

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

struct MapRemote(HashMap<Vec<String>, (i32, String, String)>);

impl Remote for MapRemote {
    fn run(&self, argv: &[&str]) -> (i32, String, String) {
        let key: Vec<String> = argv.iter().map(|part| (*part).to_string()).collect();
        self.0
            .get(&key)
            .cloned()
            .unwrap_or((1, String::new(), "missing mock".into()))
    }
}

fn recipe() -> game_library::registry::Recipe {
    parse_recipe(
        r#"
slug: example-game
title: Example Game
source: gog
installable: false
product_id: "0"
lutris:
  existing_only: true
  directory_template: "{{ gaming_workstations_gog_library_root }}/example-game"
ludusavi:
  name: Example Game
"#,
    )
    .unwrap()
}

#[test]
fn lists_unregistered_lutris_install() {
    let mut map = HashMap::new();
    map.insert(
        vec![
            "lutris".into(),
            "--list-games".into(),
            "--installed".into(),
            "--json".into(),
        ],
        (
            0,
            r#"[{"id":1,"slug":"other-game","name":"Other Game","directory":"/home/alice/.local/share/games/gog/other-game"}]"#.into(),
            String::new(),
        ),
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
        (1, String::new(), String::new()),
    );
    map.insert(
        vec![
            "loginctl".into(),
            "list-sessions".into(),
            "--no-legend".into(),
        ],
        (0, "1 alice wayland active\n".into(), String::new()),
    );
    map.insert(
        vec![
            "/home/alice/.local/bin/ludusavi-lutris-wrap".into(),
            "status".into(),
            "--name".into(),
            "Example Game".into(),
        ],
        (1, String::new(), String::new()),
    );
    map.insert(
        vec![
            "test".into(),
            "-e".into(),
            "/home/alice/.local/state/game-library/save-lineage/import-restore.json".into(),
        ],
        (1, String::new(), String::new()),
    );
    let rows = collect_library(&host(), &[recipe()], &MapRemote(map));
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].slug, "example-game");
    assert!(rows[0].recipe);
    assert!(!rows[0].installed);
    assert_eq!(rows[1].slug, "other-game");
    assert!(!rows[1].recipe);
    assert_eq!(rows[1].notes, ["unregistered"]);
}
