//! UI-independent logic: emoji data and search, settings. Keep Slint out of this crate so a CLI can reuse it.
pub mod emojis;
pub mod files;
pub mod fold;
pub mod settings;
