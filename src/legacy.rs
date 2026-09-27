#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("legacy; skipped: {what} {path}")]
pub struct LegacyTreeError {
    pub what: String,
    pub path: String,
}

pub fn is_legacy_path(path: Option<&str>, canonical: Option<&str>) -> bool {
    let Some(path) = path.filter(|value| !value.is_empty()) else {
        return false;
    };
    let resolved = normalize(path);
    if let Some(canonical) = canonical {
        if resolved == normalize(canonical) {
            return false;
        }
    }
    resolved == "/opt/soh"
        || resolved.starts_with("/opt/soh/")
        || resolved.contains("/Games/gog/")
        || resolved.ends_with("/Games/gog")
        || resolved.contains("/Applications/soh")
}

fn normalize(path: &str) -> String {
    let expanded = if let Some(rest) = path.strip_prefix("~/") {
        format!("/home/{rest}")
    } else {
        path.to_string()
    };
    let mut parts = Vec::new();
    for part in expanded.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    format!("/{}", parts.join("/"))
}

pub fn refuse_legacy(
    path: Option<&str>,
    canonical: Option<&str>,
    what: &str,
) -> Result<(), LegacyTreeError> {
    if is_legacy_path(path, canonical) {
        Err(LegacyTreeError {
            what: what.to_string(),
            path: path.unwrap_or("").to_string(),
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_xdg_is_not_legacy() {
        let path = "/home/alice/.local/share/games/gog/coromon";
        assert!(!is_legacy_path(Some(path), Some(path)));
    }

    #[test]
    fn home_games_gog_is_legacy() {
        assert!(is_legacy_path(
            Some("/home/alice/Games/gog/monster-train"),
            Some("/home/alice/.local/share/games/gog/monster-train"),
        ));
    }

    #[test]
    fn opt_soh_is_legacy() {
        assert!(is_legacy_path(Some("/opt/soh/Save"), None));
    }

    #[test]
    fn deck_applications_soh_is_legacy() {
        assert!(is_legacy_path(Some("/home/deck/Applications/soh"), None));
    }
}
