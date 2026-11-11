//! Bevy-free settings of the player: the model, defaults and versioned JSON
//! persistence.
//!
//! The file is a small object with an explicit `version`; fields the reader
//! does not know are ignored and fields it knows carry defaults, so an older
//! binary opens a newer-or-partial file without a format bump until a field
//! changes meaning (then `FILE_VERSION` moves and older binaries refuse with
//! [`LoadError::UnsupportedVersion`] instead of misreading).
//!
//! Loss-proofing of [`save`]: the new content first lands in `<name>.tmp`,
//! and the previous file is renamed to `<name>.bak` before being replaced
//! when it is unreadable (corrupt) or newer than this format — data worth
//! keeping is never silently destroyed. An existing `.bak` is rotated aside
//! (`<name>.bak.old`) *before* being replaced, so a failed rename never
//! costs the last good backup. Each step is one `rename` — atomic on one
//! volume — but the save as a whole is not a transaction: a crash mid-save
//! may leave `path.tmp` (the previous file intact), `path.tmp` + `path.bak`
//! (copy the `.bak` back), or — only during a rotation — additionally
//! `path.bak.old` (the newest previous content is `.bak`, `.bak.old` is the
//! one before it). On success the `.bak` stays as the rescued copy until the
//! next rotation replaces it; `.bak.old` is removed best-effort and may be
//! left behind by a crash — it is never read back.
//!
//! The crate knows no Bevy, no window and no game rule: applying `Settings`
//! to a running app is the frontend's job.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The format version this build reads and writes.
pub const FILE_VERSION: u32 = 1;

/// The interface language. A user preference, not a game rule.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    Russian,
    English,
}

/// Video settings that are honestly appliable today: both reach a live bevy
/// window (`Window::mode`, `Window::present_mode`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct VideoSettings {
    /// Start in fullscreen. `false` — a window.
    #[serde(default = "default_false")]
    pub fullscreen: bool,
    /// VSync on. `true`: a tearing-free frame is worth a latency step here.
    #[serde(default = "default_true")]
    pub vsync: bool,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            fullscreen: false,
            vsync: true,
        }
    }
}

/// The settings of the player application.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub language: Language,
    #[serde(default)]
    pub video: VideoSettings,
}

/// The on-disk shape: everything of [`Settings`] plus the version key.
#[derive(Serialize, Deserialize)]
struct SettingsFile {
    version: u32,
    #[serde(flatten)]
    settings: Settings,
}

/// The defaults a first run and a lost file fall back to.
pub fn defaults() -> Settings {
    Settings::default()
}

/// Where the settings live when the caller passes no path:
/// `dirs::config_dir()/re-konung/settings.json`. `None` when the OS config
/// directory is unresolvable — the caller then runs on defaults and a save
/// attempt will fail with a typed error instead of writing somewhere random.
/// The exe directory is deliberately NOT a fallback: it is often read-only.
pub fn default_path() -> Option<PathBuf> {
    dirs::config_dir().map(|config| config.join("re-konung").join("settings.json"))
}

/// Why a settings file did not load. `Missing` is a first run, not a failure;
/// everything else leaves the file untouched and the app on defaults.
#[derive(Debug)]
pub enum LoadError {
    /// No file at the path yet.
    Missing,
    /// The bytes are not a settings file this code recognizes.
    Corrupted,
    /// The file is of a newer format version.
    UnsupportedVersion(u32),
    /// Reading failed.
    Io(std::io::Error),
}

/// Why a save failed, with the io detail for the caller's log. `Write` leaves
/// the previous file untouched; `Replace` leaves it at `path` (or restored
/// from the backup) — neither ever loses it.
#[derive(Debug)]
pub enum SaveError {
    Write(std::io::Error),
    Replace(std::io::Error),
}

/// Reads the settings. Never writes: a `Corrupted` or `UnsupportedVersion`
/// file stays as it is until an explicit [`save`] (which backs it up first).
pub fn load(path: &Path) -> Result<Settings, LoadError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(LoadError::Missing);
        }
        Err(error) => return Err(LoadError::Io(error)),
    };
    parse(&bytes)
}

/// Writes the settings: `create_dir_all` on the parent, the new content into
/// `<name>.tmp`, a backup of an unreadable-or-newer previous file, then the
/// rename over `path` (`std::fs::rename` replaces an existing destination on
/// both Unix and Windows — on Windows via `MoveFileEx` with
/// `MOVEFILE_REPLACE_EXISTING`, which fails on a destination held open or
/// read-only; that failure is the `Replace` kind, and the backup is rolled
/// back onto `path`).
pub fn save(path: &Path, settings: &Settings) -> Result<(), SaveError> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(SaveError::Write)?;
    }
    let json = serde_json::to_string_pretty(&SettingsFile {
        version: FILE_VERSION,
        settings: *settings,
    })
    .map_err(|error| SaveError::Write(error.into()))?;

    let tmp = sibling(path, ".tmp");
    let bak = sibling(path, ".bak");
    let bak_old = sibling(path, ".bak.old");
    let write = || -> std::io::Result<()> {
        use std::io::Write;
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()
    };
    if let Err(error) = write() {
        let _ = std::fs::remove_file(&tmp);
        return Err(SaveError::Write(error));
    }

    let backed_up = needs_backup(path);
    let mut rotated = false;
    if backed_up {
        // Rotate the previous backup aside before replacing it: if the rename
        // below failed after an unconditional remove, the last good backup
        // would be lost. With the rotation, every failure path leaves the
        // old backup where it was (or restored).
        if bak.exists() {
            if let Err(error) = std::fs::rename(&bak, &bak_old) {
                let _ = std::fs::remove_file(&tmp);
                return Err(SaveError::Replace(error));
            }
            rotated = true;
        }
        if let Err(error) = std::fs::rename(path, &bak) {
            if rotated {
                // Put the previous backup back where it was.
                let _ = std::fs::rename(&bak_old, &bak);
            }
            let _ = std::fs::remove_file(&tmp);
            return Err(SaveError::Replace(error));
        }
    }
    match std::fs::rename(&tmp, path) {
        Ok(()) => {
            if rotated {
                // The rotated copy is superseded by the fresh `.bak`; left
                // behind only when this removal fails or the process dies.
                let _ = std::fs::remove_file(&bak_old);
            }
            Ok(())
        }
        Err(error) => {
            if backed_up {
                // Best effort: put the rescued file back where it was, and
                // the rotated one behind it.
                let _ = std::fs::rename(&bak, path);
                if rotated {
                    let _ = std::fs::rename(&bak_old, &bak);
                }
            }
            Err(SaveError::Replace(error))
        }
    }
}

/// A sibling path: `foo.json` + `.tmp` → `foo.json.tmp`.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// Whether the existing file must be preserved before being replaced: it is
/// anything other than a settings file this code fully understands — corrupt,
/// unreadable, or of a newer version. A valid current-or-older file is being
/// legitimately updated, not destroyed.
fn needs_backup(path: &Path) -> bool {
    match std::fs::read(path) {
        Ok(bytes) => existing_needs_backup(&bytes),
        // No previous file: nothing to preserve. Anything else unreadable is
        // present but inaccessible — preserve it rather than judge it.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => true,
    }
}

/// The backup decision on bytes, separate from the filesystem so it is
/// directly testable. Everything `parse` refuses — corrupt, no version, or a
/// newer version, whose fields this code cannot even name — is preserved.
fn existing_needs_backup(bytes: &[u8]) -> bool {
    parse(bytes).is_err()
}

/// The reader: bytes → version gate → [`Settings`]. Shape failures are
/// `Corrupted`; only the version check is its own kind.
fn parse(bytes: &[u8]) -> Result<Settings, LoadError> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| LoadError::Corrupted)?;
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(LoadError::Corrupted)?;
    if version > FILE_VERSION as u64 {
        return Err(LoadError::UnsupportedVersion(version as u32));
    }
    serde_json::from_value(value).map_err(|_| LoadError::Corrupted)
}

fn default_false() -> bool {
    false
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(dir: &tempfile::TempDir, name: &str) -> PathBuf {
        dir.path().join(name)
    }

    #[test]
    fn defaults_start_windowed_with_vsync_on() {
        let defaults = defaults();
        assert_eq!(defaults.language, Language::Russian);
        assert!(!defaults.video.fullscreen);
        assert!(defaults.video.vsync);
    }

    #[test]
    fn round_trips_through_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = path(&dir, "settings.json");
        let saved = Settings {
            language: Language::English,
            video: VideoSettings {
                fullscreen: true,
                vsync: false,
            },
        };
        save(&path, &saved).unwrap();
        assert_eq!(load(&path).unwrap(), saved);
        // The written shape is the documented one
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"version\": 1"));
        assert!(text.contains("\"language\": \"english\""));
        assert!(!path.with_extension("json.bak").exists());
    }

    #[test]
    fn a_missing_file_is_reported_as_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            load(&path(&dir, "settings.json")),
            Err(LoadError::Missing)
        ));
    }

    #[test]
    fn garbage_is_corrupted() {
        let dir = tempfile::tempdir().unwrap();
        let path = path(&dir, "settings.json");
        std::fs::write(&path, b"not json at all").unwrap();
        assert!(matches!(load(&path), Err(LoadError::Corrupted)));
    }

    #[test]
    fn json_of_the_wrong_shape_is_corrupted() {
        let dir = tempfile::tempdir().unwrap();
        let path = path(&dir, "settings.json");
        std::fs::write(&path, br#"{"version": 1, "video": "big"}"#).unwrap();
        assert!(matches!(load(&path), Err(LoadError::Corrupted)));
    }

    #[test]
    fn a_newer_version_is_refused_with_its_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = path(&dir, "settings.json");
        std::fs::write(&path, br#"{"version": 2, "language": "english"}"#).unwrap();
        assert!(matches!(load(&path), Err(LoadError::UnsupportedVersion(2))));
    }

    #[test]
    fn a_missing_version_is_corrupted() {
        let dir = tempfile::tempdir().unwrap();
        let path = path(&dir, "settings.json");
        std::fs::write(&path, br#"{"language": "english"}"#).unwrap();
        assert!(matches!(load(&path), Err(LoadError::Corrupted)));
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = path(&dir, "settings.json");
        std::fs::write(
            &path,
            br#"{"version": 1, "future": {"nested": true}, "language": "english"}"#,
        )
        .unwrap();
        assert_eq!(load(&path).unwrap().language, Language::English);
    }

    #[test]
    fn a_partial_file_defaults_its_missing_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = path(&dir, "settings.json");
        std::fs::write(&path, br#"{"version": 1}"#).unwrap();
        assert_eq!(load(&path).unwrap(), defaults());
    }

    #[test]
    fn load_never_writes_a_corrupt_file_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = path(&dir, "settings.json");
        std::fs::write(&path, b"not json at all").unwrap();
        assert!(load(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"not json at all".as_slice());
    }

    #[test]
    fn saving_over_a_valid_file_replaces_it_without_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = path(&dir, "settings.json");
        save(&path, &defaults()).unwrap();
        let next = Settings {
            language: Language::English,
            ..defaults()
        };
        save(&path, &next).unwrap();
        assert_eq!(load(&path).unwrap(), next);
        assert!(!path.with_extension("json.bak").exists());
    }

    #[test]
    fn saving_over_a_corrupt_file_backs_it_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = path(&dir, "settings.json");
        let old = b"not json at all";
        std::fs::write(&path, old).unwrap();
        save(&path, &defaults()).unwrap();
        assert_eq!(std::fs::read(path.with_extension("json.bak")).unwrap(), old);
        assert_eq!(load(&path).unwrap(), defaults());
    }

    #[test]
    fn repeated_saves_rotate_the_backup_keeping_the_newest_previous_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = path(&dir, "settings.json");
        let first = b"first corrupt file";
        let second = b"second corrupt file";
        std::fs::write(&path, first).unwrap();
        save(&path, &defaults()).unwrap();
        std::fs::write(&path, second).unwrap();
        save(&path, &defaults()).unwrap();
        // The fresh backup is the newest replaced content; the rotated one is
        // gone — superseded, not kept.
        assert_eq!(
            std::fs::read(path.with_extension("json.bak")).unwrap(),
            second
        );
        assert!(!path.with_extension("json.bak.old").exists());
        assert_eq!(load(&path).unwrap(), defaults());
    }

    #[test]
    fn a_failed_rotation_keeps_the_previous_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = path(&dir, "settings.json");
        let corrupt = b"corrupt current file";
        let previous_backup = b"the last good backup";
        std::fs::write(&path, corrupt).unwrap();
        std::fs::write(path.with_extension("json.bak"), previous_backup).unwrap();
        // A non-empty directory where the rotated backup would land makes the
        // rotation rename fail portably (ENOTEMPTY / ERROR_DIR_NOT_EMPTY),
        // which is the step that used to run after an unconditional remove
        // and could cost the old backup.
        let blocked = path.with_extension("json.bak.old");
        std::fs::create_dir(&blocked).unwrap();
        std::fs::write(blocked.join("occupied"), b"x").unwrap();

        assert!(matches!(
            save(&path, &defaults()),
            Err(SaveError::Replace(_))
        ));

        // Nothing was lost: the current file, the previous backup and the
        // blocker are all untouched; no tmp leftovers.
        assert_eq!(std::fs::read(&path).unwrap(), corrupt);
        assert_eq!(
            std::fs::read(path.with_extension("json.bak")).unwrap(),
            previous_backup
        );
        assert!(blocked.join("occupied").exists());
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn saving_over_a_newer_file_backs_it_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = path(&dir, "settings.json");
        let newer = br#"{"version": 7, "language": "english", "future": 1}"#;
        std::fs::write(&path, newer).unwrap();
        save(&path, &defaults()).unwrap();
        assert_eq!(
            std::fs::read(path.with_extension("json.bak")).unwrap(),
            newer.as_slice()
        );
        assert_eq!(load(&path).unwrap(), defaults());
    }

    #[test]
    fn a_failed_write_leaves_the_previous_file_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = path(&dir, "settings.json");
        save(&path, &defaults()).unwrap();
        // A directory where the tmp file would be created makes the write
        // fail without touching permissions (unportable to set).
        std::fs::create_dir(path.with_extension("json.tmp")).unwrap();
        let next = Settings {
            video: VideoSettings {
                fullscreen: true,
                vsync: false,
            },
            ..defaults()
        };
        assert!(matches!(save(&path, &next), Err(SaveError::Write(_))));
        assert_eq!(load(&path).unwrap(), defaults());
    }

    #[test]
    fn the_backup_decision_covers_every_byte_shape() {
        let valid = br#"{"version": 1, "language": "english"}"#;
        let older_missing_fields = br#"{"version": 1}"#;
        let partial_wrong_shape = br#"{"version": 1, "video": 3}"#;
        let newer = br#"{"version": 2}"#;
        let garbage = b"not json";
        let empty = b"";
        assert!(!existing_needs_backup(valid));
        assert!(!existing_needs_backup(older_missing_fields));
        assert!(existing_needs_backup(partial_wrong_shape));
        assert!(existing_needs_backup(newer));
        assert!(existing_needs_backup(garbage));
        assert!(existing_needs_backup(empty));
    }

    #[test]
    fn the_default_path_is_inside_re_konung_when_it_exists() {
        if let Some(path) = default_path() {
            let text = path.to_str().expect("a UTF-8 config path");
            let normalized = text.replace('\\', "/");
            assert!(normalized.ends_with("re-konung/settings.json"));
        }
    }
}
