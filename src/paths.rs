use std::path::{Path, PathBuf};

pub fn inventory_repo(cwd: Option<&Path>) -> PathBuf {
    if let Ok(repo) = std::env::var("GAME_LIBRARY_REPO") {
        return PathBuf::from(repo);
    }
    if let Some(cwd) = cwd {
        if cwd.join("registry/games").is_dir() || cwd.join("examples/hosts.yaml").is_file() {
            return cwd.to_path_buf();
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn registry_root() -> PathBuf {
    if let Ok(root) = std::env::var("GAME_LIBRARY_REGISTRY") {
        return PathBuf::from(root);
    }
    let from_repo = inventory_repo(std::env::current_dir().ok().as_deref()).join("registry/games");
    if from_repo.is_dir() {
        return from_repo;
    }
    if let Ok(home) = std::env::var("HOME") {
        let xdg = PathBuf::from(home).join(".local/share/game-library/registry/games");
        if xdg.is_dir() {
            return xdg;
        }
    }
    from_repo
}

pub fn current_username() -> Option<String> {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .ok()
        .filter(|value| !value.is_empty())
}

pub fn current_hostname() -> Option<String> {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn is_local_host(host_name: &str, desktop_user: &str, user: &str, hostname: &str) -> bool {
    host_name == hostname && desktop_user == user
}

pub fn inventory_present(repo: &Path) -> bool {
    repo.join("registry/games").is_dir()
}
