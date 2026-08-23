use serde::{Deserialize, Serialize};

use crate::inventory::GamingHost;
use crate::registry::Recipe;
use crate::status::{collect_status, lutris_installed_games, Remote};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LibraryRow {
    pub slug: String,
    pub title: String,
    pub source: String,
    pub recipe: bool,
    pub installed: bool,
    pub enrolled: bool,
    pub complete: bool,
    pub legacy: bool,
    pub live_directory: Option<String>,
    pub notes: Vec<String>,
}

pub fn collect_library(
    host: &GamingHost,
    recipes: &[Recipe],
    remote: &dyn Remote,
) -> Vec<LibraryRow> {
    let mut rows: Vec<LibraryRow> = recipes
        .iter()
        .map(|recipe| {
            let status = collect_status(host, recipe, remote);
            LibraryRow {
                slug: recipe.slug.clone(),
                title: recipe.title.clone(),
                source: format!("{:?}", recipe.source).to_ascii_lowercase(),
                recipe: true,
                installed: status.installed,
                enrolled: status.enrolled,
                complete: status.complete,
                legacy: status.legacy,
                live_directory: status.live_directory,
                notes: status.notes,
            }
        })
        .collect();
    let known_slugs: Vec<String> = recipes.iter().map(|recipe| recipe.slug.clone()).collect();
    let known_titles: Vec<String> = recipes.iter().map(|recipe| recipe.title.clone()).collect();
    for game in lutris_installed_games(remote) {
        let slug = if game.slug.is_empty() {
            continue;
        } else {
            game.slug
        };
        if known_slugs.iter().any(|known| known == &slug)
            || known_titles.iter().any(|title| title == &game.name)
        {
            continue;
        }
        rows.push(LibraryRow {
            slug,
            title: game.name,
            source: "lutris".into(),
            recipe: false,
            installed: true,
            enrolled: false,
            complete: false,
            legacy: crate::legacy::is_legacy_path(game.directory.as_deref(), None),
            live_directory: game.directory,
            notes: vec!["unregistered".into()],
        });
    }
    rows.sort_by(|left, right| left.slug.cmp(&right.slug));
    rows
}

pub fn format_rows(host: &str, rows: &[LibraryRow]) -> String {
    let mut lines = vec![
        format!("host: {host}"),
        "slug\trecipe\tinstalled\tenrolled\tcomplete\tnotes".into(),
    ];
    if rows.is_empty() {
        lines.push("(none)".into());
        return lines.join("\n");
    }
    for row in rows {
        let notes = if row.notes.is_empty() {
            "-".into()
        } else {
            row.notes.join(",")
        };
        lines.push(format!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            row.slug,
            yn(row.recipe),
            yn(row.installed),
            yn(row.enrolled),
            yn(row.complete),
            notes
        ));
    }
    lines.join("\n")
}

fn yn(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}
