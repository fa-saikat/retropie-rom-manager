//! What the app remembers between launches, in
//! `~/.config/retropie-rom-manager/`:
//!
//! - `systems`: which systems the user keeps in the sidebar, one id per
//!   line. A system that has games on disk is always shown whether it's
//!   listed or not (see `RootView::refresh`), so this file only has to
//!   remember the ones added while still empty.
//! - `theme`: `light` or `dark`, the theme last picked in the app (#57).
//!
//! Like `library`, no GPUI dependency, so it's unit tested with `cargo test`.

use anyhow::{anyhow, Result};
use std::fs;
use std::path::PathBuf;

use crate::cli::Theme;
use crate::systems::{self, DEFAULT_ENABLED};

fn file(name: &str) -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("retropie-rom-manager").join(name))
}

fn path() -> Option<PathBuf> {
    file("systems")
}

fn write(path: Option<PathBuf>, text: &str) -> Result<()> {
    let path = path.ok_or_else(|| anyhow!("no config folder found"))?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(&path, text)?;
    Ok(())
}

/// Known ids from `text`, in file order, without duplicates.
fn parse(text: &str) -> Vec<&'static str> {
    let mut ids: Vec<&'static str> = Vec::new();
    for line in text.lines().map(str::trim) {
        if let Some(system) = systems::by_id(line) {
            if !ids.contains(&system.id) {
                ids.push(system.id);
            }
        }
    }
    ids
}

/// The user's saved list, or the defaults if they've never changed it.
pub fn load_enabled() -> Vec<&'static str> {
    match path().and_then(|p| fs::read_to_string(p).ok()) {
        // An emptied-out file means "start over", not "show nothing".
        Some(text) if !parse(&text).is_empty() => parse(&text),
        _ => DEFAULT_ENABLED.to_vec(),
    }
}

pub fn save_enabled(ids: &[&str]) -> Result<()> {
    write(path(), &ids.iter().map(|id| format!("{id}\n")).collect::<String>())
}

fn parse_theme(text: &str) -> Option<Theme> {
    match text.trim() {
        "light" => Some(Theme::Light),
        "dark" => Some(Theme::Dark),
        _ => None,
    }
}

/// The theme last picked in the app, if any.
pub fn load_theme() -> Option<Theme> {
    file("theme").and_then(|p| fs::read_to_string(p).ok()).as_deref().and_then(parse_theme)
}

pub fn save_theme(theme: Theme) -> Result<()> {
    write(file("theme"), if theme == Theme::Dark { "dark\n" } else { "light\n" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_known_ids_in_order_once() {
        assert_eq!(parse("snes\n  gb \nnot-a-system\nsnes\n\n"), ["snes", "gb"]);
        assert!(parse("").is_empty());
    }

    #[test]
    fn reads_the_theme() {
        assert_eq!(parse_theme("dark\n"), Some(Theme::Dark));
        assert_eq!(parse_theme(" light "), Some(Theme::Light));
        assert_eq!(parse_theme("blue"), None);
        assert_eq!(parse_theme(""), None);
    }
}
