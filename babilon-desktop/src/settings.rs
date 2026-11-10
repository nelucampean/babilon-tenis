//! Settings persistence across restarts: difficulty, audio mode, volume
//! group levels, and the stereo-swap flag live in a small TOML file
//! (`~/.config/BabilonTennis/settings.toml` on Linux,
//! `%APPDATA%\Babilon\BabilonTennis\config\settings.toml` on Windows,
//! `./babilon-settings.toml` fallback). Missing or corrupt file = defaults.
//! Saves never panic the game; failures are logged to stderr.

use crate::audio::{AudioMode, Volumes};
use babilon_core::Difficulty;
use bevy::prelude::*;
use std::path::PathBuf;

/// Every user preference in one place: difficulty, audio mode, the three
/// volume groups, and the stereo-swap flag. Mutate this resource and let
/// [`autosave`] write it to disk; no system persists fields by hand.
#[derive(Resource, Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Settings {
    pub(crate) difficulty: Difficulty,
    pub(crate) mode: AudioMode,
    pub(crate) vols: Volumes,
    pub(crate) swap_stereo: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            difficulty: Difficulty::Normal,
            mode: AudioMode::Access,
            vols: Volumes::default(),
            swap_stereo: false,
        }
    }
}

impl Settings {
    /// Load last session's preferences, or defaults if the file is missing.
    pub(crate) fn load() -> Self {
        load_from(&settings_path())
    }

    /// Best-effort save to the platform config path.
    pub(crate) fn save(&self) {
        save_to(&settings_path(), self);
    }
}

/// Persist once per frame in which [`Settings`] actually changed.
pub(crate) fn autosave(settings: Res<Settings>) {
    if settings.is_changed() && !settings.is_added() {
        settings.save();
    }
}

fn settings_path() -> PathBuf {
    directories::ProjectDirs::from("", "Babilon", "BabilonTennis")
        .map(|p| p.config_dir().join("settings.toml"))
        .unwrap_or_else(|| PathBuf::from("babilon-settings.toml"))
}

fn load_from(path: &std::path::Path) -> Settings {
    match std::fs::read_to_string(path) {
        Ok(text) => match toml::from_str(&text) {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "[SETUP] ignoring corrupt settings file {}: {e}",
                    path.display()
                );
                Settings::default()
            }
        },
        Err(_) => Settings::default(),
    }
}

fn save_to(path: &std::path::Path, saved: &Settings) {
    let text = match toml::to_string_pretty(saved) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[SETUP] cannot serialize settings: {e}");
            return;
        }
    };
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    if let Err(e) = std::fs::write(path, text) {
        eprintln!("[SETUP] cannot write {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_through_toml() {
        let dir = std::env::temp_dir().join(format!("babilon-test-{}", std::process::id()));
        let path = dir.join("settings.toml");
        let _ = std::fs::remove_dir_all(&dir);
        // missing file -> defaults, no panic
        assert_eq!(load_from(&path), Settings::default());
        let custom = Settings {
            difficulty: Difficulty::Hard,
            mode: AudioMode::Real,
            vols: Volumes {
                ball: 1.2,
                umpire: 0.5,
                crowd: 0.0,
                group: crate::audio::VolGroup::Crowd,
            },
            swap_stereo: true,
        };
        save_to(&path, &custom);
        assert_eq!(load_from(&path), custom);
        // corrupt file -> defaults, no panic
        std::fs::write(&path, "difficulty = [unclosed").unwrap();
        assert_eq!(load_from(&path), Settings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
