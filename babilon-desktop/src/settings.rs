//! Settings persistence across restarts: difficulty, audio mode, volume
//! group levels, and the stereo-swap flag live in a small TOML file
//! (`~/.config/BabilonTennis/settings.toml` on Linux,
//! `%APPDATA%\Babilon\BabilonTennis\config\settings.toml` on Windows,
//! `./babilon-settings.toml` fallback). Missing or corrupt file = defaults.
//! Saves never panic the game; failures are logged to stderr.

use crate::audio::{AudioMode, Volumes};
use babilon_core::Difficulty;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Persisted {
    pub difficulty: Difficulty,
    pub mode: AudioMode,
    pub vols: Volumes,
    pub swap_stereo: bool,
}

impl Default for Persisted {
    fn default() -> Self {
        Self {
            difficulty: Difficulty::Normal,
            mode: AudioMode::Access,
            vols: Volumes::default(),
            swap_stereo: false,
        }
    }
}

fn settings_path() -> PathBuf {
    directories::ProjectDirs::from("", "Babilon", "BabilonTennis")
        .map(|p| p.config_dir().join("settings.toml"))
        .unwrap_or_else(|| PathBuf::from("babilon-settings.toml"))
}

pub fn load() -> Persisted {
    load_from(&settings_path())
}

fn load_from(path: &std::path::Path) -> Persisted {
    match std::fs::read_to_string(path) {
        Ok(text) => match toml::from_str(&text) {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "[SETUP] ignoring corrupt settings file {}: {e}",
                    path.display()
                );
                Persisted::default()
            }
        },
        Err(_) => Persisted::default(),
    }
}

/// Snapshot the live values to disk. Best-effort: logs and returns on error.
pub fn save(difficulty: Difficulty, mode: AudioMode, vols: &Volumes, swap_stereo: bool) {
    save_to(
        &settings_path(),
        &Persisted {
            difficulty,
            mode,
            vols: *vols,
            swap_stereo,
        },
    );
}

fn save_to(path: &std::path::Path, saved: &Persisted) {
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
        assert_eq!(load_from(&path), Persisted::default());
        let custom = Persisted {
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
        assert_eq!(load_from(&path), Persisted::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
