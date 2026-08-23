use std::path::PathBuf;

use tempfile::TempDir;

use game_library::config::with_locked_env;

#[test]
fn registry_env_wins() {
    with_locked_env(&[("GAME_LIBRARY_REGISTRY", Some("/tmp/recipes"))], || {
        assert_eq!(
            game_library::paths::registry_root(),
            PathBuf::from("/tmp/recipes")
        );
    });
}

#[test]
fn registry_follows_repo_env() {
    with_locked_env(
        &[
            ("GAME_LIBRARY_REGISTRY", None),
            ("GAME_LIBRARY_REPO", Some("/tmp/public-game-library")),
        ],
        || {
            assert_eq!(
                game_library::paths::registry_root(),
                PathBuf::from("/tmp/public-game-library/registry/games")
            );
        },
    );
}

#[test]
fn inventory_uses_cwd_when_it_looks_like_the_repo() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("registry/games")).unwrap();
    with_locked_env(
        &[("GAME_LIBRARY_REPO", None), ("GAME_LIBRARY_REGISTRY", None)],
        || {
            assert_eq!(
                game_library::paths::inventory_repo(Some(tmp.path())),
                tmp.path()
            );
        },
    );
}

#[test]
fn local_host_requires_matching_user_and_hostname() {
    assert!(game_library::paths::is_local_host(
        "laptop", "alice", "alice", "laptop"
    ));
    assert!(!game_library::paths::is_local_host(
        "laptop", "alice", "bob", "laptop"
    ));
}
