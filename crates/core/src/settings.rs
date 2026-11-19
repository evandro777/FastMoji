//! User settings in one TOML file under the XDG data dir (`~/.local/share/FastMoji`, or the Flatpak data dir).
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    emojis,
    files::{read_or_backup, write_atomic},
};

pub const MAX_RECENT: usize = 60;
pub const EMOJI_SIZES: [u32; 5] = [20, 24, 28, 32, 36];
pub const MIN_SIZE: (u32, u32) = (356, 300);
/// Upper bound for a hand-edited size; a huge value would fail at window creation.
pub const MAX_SIZE: u32 = 8192;
const DEFAULT_RECENT: [&str; 9] = ["🙂", "😄", "❤️", "👍", "🤞", "🔥", "🤣", "😍", "😭"];

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub recent: Vec<String>,
    pub skin_tone: usize,
    pub emoji_size: u32,
    pub auto_paste: bool,
    /// Keep the process running when the picker is closed (needed for the global shortcut).
    /// When false, FastMoji exits after closing, as soon as nothing needs the clipboard any more.
    pub keep_resident: bool,
    /// e.g. "Ctrl+Alt+.". Also editable in Preferences.
    pub shortcut: String,
    /// "" follows the system locale; otherwise a code from `emojis::language_codes()` or "en".
    pub language: String,
    /// "" follows the system's light or dark mode; otherwise "light" or "dark".
    pub theme: String,
    pub width: u32,
    pub height: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            recent: DEFAULT_RECENT.iter().map(|s| s.to_string()).collect(),
            skin_tone: 0,
            emoji_size: 28,
            auto_paste: true,
            keep_resident: true,
            shortcut: "Ctrl+Alt+.".into(),
            language: String::new(),
            theme: String::new(),
            width: 515,
            height: 500,
        }
    }
}

fn parse(text: &str) -> Result<Settings, toml::de::Error> {
    toml::from_str(text)
}

fn data_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("FastMoji")
}

impl Settings {
    pub fn load() -> Self {
        Self::load_from(&data_dir())
    }

    fn load_from(dir: &Path) -> Self {
        // The file is hand-edited; a bad one is moved aside so the next save() must not overwrite what the user wrote.
        let mut s = read_or_backup(&dir.join("settings.toml"), |t| parse(t).map_err(|e| e.to_string())).0;
        s.normalize();
        s
    }

    fn normalize(&mut self) {
        self.skin_tone = self.skin_tone.min(5);
        // Snap to the nearest supported size, like `normalize_emoji_size` in Python.
        self.emoji_size = *EMOJI_SIZES.iter().min_by_key(|s| s.abs_diff(self.emoji_size)).unwrap();
        self.width = self.width.clamp(MIN_SIZE.0, MAX_SIZE);
        self.height = self.height.clamp(MIN_SIZE.1, MAX_SIZE);
        self.recent.truncate(MAX_RECENT);
    }

    /// Atomic write so a crash can never leave a half-written file. Failures (full disk, read-only
    /// home) are reported on stderr; the picker keeps working with the in-memory settings.
    pub fn save(&self) {
        let dir = data_dir();
        if let Err(e) = self.try_save(&dir) {
            eprintln!("cannot save settings to {}: {e}", dir.display());
        }
    }

    fn try_save(&self, dir: &Path) -> std::io::Result<()> {
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        write_atomic(&dir.join("settings.toml"), text)
    }

    /// Move `ch` to the front of the recents, keeping at most `MAX_RECENT`.
    pub fn push_recent(&mut self, ch: &str) {
        let ch = emojis::strip_tone(ch);
        self.recent.retain(|c| *c != ch);
        self.recent.truncate(MAX_RECENT - 1);
        self.recent.insert(0, ch);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn recents_move_to_front_without_duplicates() {
        let mut s = Settings::default();
        s.push_recent("🔥");
        assert_eq!(s.recent[0], "🔥");
        assert_eq!(s.recent.iter().filter(|c| *c == "🔥").count(), 1);
    }

    #[test]
    fn recents_are_capped_and_tone_stripped() {
        let mut s = Settings::default();
        for i in 0..100u32 {
            s.push_recent(&char::from_u32(0x1F600 + i).unwrap().to_string());
        }
        assert_eq!(s.recent.len(), MAX_RECENT);
        s.push_recent("👍🏽");
        assert_eq!(s.recent[0], "👍");
    }

    #[test]
    fn repicking_in_a_full_list_keeps_it_full() {
        let mut s = Settings::default();
        for i in 0..MAX_RECENT as u32 {
            s.push_recent(&char::from_u32(0x1F600 + i).unwrap().to_string());
        }
        assert_eq!(s.recent.len(), MAX_RECENT);
        let oldest = s.recent[MAX_RECENT - 1].clone();
        s.push_recent(&oldest);
        assert_eq!(s.recent.len(), MAX_RECENT);
        assert_eq!(s.recent[0], oldest);
    }

    #[test]
    fn resident_by_default_and_old_files_still_load() {
        assert!(Settings::default().keep_resident);
        // A settings file written before the option existed must not turn residency off.
        let parsed: Settings = toml::from_str("auto_paste = false").unwrap();
        assert!(parsed.keep_resident);
        assert!(!parsed.auto_paste);
    }

    #[test]
    fn language_defaults_to_system_for_old_files() {
        let parsed: Settings = toml::from_str("auto_paste = false").unwrap();
        assert_eq!(parsed.language, "");
        let parsed: Settings = toml::from_str("language = \"fr\"").unwrap();
        assert_eq!(parsed.language, "fr");
    }

    #[test]
    fn theme_defaults_to_system_for_old_files() {
        let parsed: Settings = toml::from_str("auto_paste = false").unwrap();
        assert_eq!(parsed.theme, "");
        let parsed: Settings = toml::from_str("theme = \"dark\"").unwrap();
        assert_eq!(parsed.theme, "dark");
    }

    #[test]
    fn invalid_file_is_an_error_not_defaults() {
        assert!(parse("emoji_size = \"big\"").is_err());
    }

    #[test]
    fn saved_settings_reload_and_a_broken_file_is_kept_aside() {
        let dir = std::env::temp_dir().join(format!("fastmoji-settings-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let saved = Settings {
            auto_paste: false,
            shortcut: "Ctrl+Alt+Y".into(),
            ..Default::default()
        };
        saved.try_save(&dir).unwrap();
        let loaded = Settings::load_from(&dir);
        assert!(!loaded.auto_paste);
        assert_eq!(loaded.shortcut, "Ctrl+Alt+Y");

        fs::write(dir.join("settings.toml"), "emoji_size = \"big\"").unwrap();
        assert_eq!(Settings::load_from(&dir).emoji_size, 28, "defaults");
        let backup = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .find(|e| e.file_name().to_string_lossy().starts_with("settings.toml.bad."))
            .expect("the broken file is kept aside");
        assert_eq!(fs::read_to_string(backup.path()).unwrap(), "emoji_size = \"big\"");
        assert!(
            !dir.join("settings.toml").exists(),
            "moved, so the next save cannot overwrite it"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn size_snaps_to_supported_value() {
        let mut s = Settings {
            emoji_size: 29,
            ..Default::default()
        };
        s.normalize();
        assert_eq!(s.emoji_size, 28);
    }

    #[test]
    fn window_size_is_clamped_both_ways() {
        let mut s: Settings = toml::from_str("width = 4000000000\nheight = 1").unwrap();
        s.normalize();
        assert_eq!((s.width, s.height), (MAX_SIZE, MIN_SIZE.1));
    }
}
