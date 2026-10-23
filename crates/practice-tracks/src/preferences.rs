//! A journaled, temporary change of MuseScore's native metronome preference.
use crate::Result;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const KEY: &str = "application/playback/metronomeEnabled";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Store {
    Ini(PathBuf),
    #[cfg(target_os = "macos")]
    Defaults,
}

#[derive(Debug, Serialize, Deserialize)]
struct Journal {
    store: Store,
    original: Option<String>,
    applied: String,
}

pub(crate) struct MetronomePreference {
    _lock: File,
    journal_path: PathBuf,
    journal: Journal,
    restored: bool,
}

impl MetronomePreference {
    pub(crate) fn begin(recovery_dir: &Path, enabled: bool) -> Result<Self> {
        Self::begin_store(recovery_dir, Store::discover()?, enabled)
    }

    fn begin_store(recovery_dir: &Path, store: Store, enabled: bool) -> Result<Self> {
        fs::create_dir_all(recovery_dir)
            .map_err(|e| format!("Cannot create preference recovery directory: {e}"))?;
        let lock_path = match &store {
            Store::Ini(path) => path
                .parent()
                .ok_or("Invalid MuseScore preference path")?
                .join(".voiceweaver-metronome.lock"),
            #[cfg(target_os = "macos")]
            Store::Defaults => recovery_dir.join("musescore-metronome.lock"),
        };
        fs::create_dir_all(
            lock_path
                .parent()
                .ok_or("Invalid MuseScore preference lock path")?,
        )
        .map_err(|e| e.to_string())?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .map_err(|e| format!("Cannot open metronome lock: {e}"))?;
        lock.try_lock().map_err(|_| "Another VoiceWeaver practice export is using MuseScore preferences. Wait for it to finish.".to_string())?;
        let journal_path = recovery_dir.join("musescore-metronome-recovery.json");
        if journal_path.exists() {
            let bytes = read_regular_file(&journal_path, 16 * 1024 * 1024)
                .map_err(|e| format!("Cannot read metronome recovery journal: {e}"))?;
            let old: Journal = serde_json::from_slice(&bytes).map_err(|e| format!("Invalid metronome recovery journal; restore MuseScore settings manually before removing {}: {e}", journal_path.display()))?;
            if old.store != store {
                return Err("MuseScore preference location changed while a recovery journal exists. Restore the recorded preference before exporting.".into());
            }
            restore_journal(&old)?;
            remove_journal(&journal_path)?;
        }
        let original = store.read()?;
        let applied = match &store {
            Store::Ini(_) => {
                if enabled {
                    "true"
                } else {
                    "false"
                }
            }
            #[cfg(target_os = "macos")]
            Store::Defaults => {
                if enabled {
                    "1"
                } else {
                    "0"
                }
            }
        }
        .to_owned();
        let journal = Journal {
            store,
            original,
            applied,
        };
        atomic_write(
            &journal_path,
            &serde_json::to_vec(&journal).map_err(|e| e.to_string())?,
        )?;
        let mut guard = Self {
            _lock: lock,
            journal_path,
            journal,
            restored: false,
        };
        if let Err(error) = guard.journal.store.write(Some(&guard.journal.applied)) {
            // The durable journal remains available even if a platform write partially failed.
            return match guard.restore() {
                Ok(()) => Err(error),
                Err(restoration) => Err(format!("{error}\n{restoration}")),
            };
        }
        if guard.journal.store.read()? != Some(guard.journal.applied.clone()) {
            return Err("MuseScore's native metronome preference could not be verified. Rendering was stopped.".into());
        }
        Ok(guard)
    }

    pub(crate) fn restore(&mut self) -> Result<()> {
        if self.restored {
            return Ok(());
        }
        restore_journal(&self.journal)?;
        remove_journal(&self.journal_path)?;
        self.restored = true;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn begin_test(recovery_dir: &Path, path: &Path, enabled: bool) -> Result<Self> {
        Self::begin_store(recovery_dir, Store::Ini(path.to_path_buf()), enabled)
    }
}

impl Drop for MetronomePreference {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

fn restore_journal(journal: &Journal) -> Result<()> {
    let current = journal.store.read()?;
    if current == journal.original {
        return Ok(());
    }
    if current.as_deref() != Some(journal.applied.as_str()) {
        return Err("MuseScore's metronome preference changed during rendering. It was not overwritten; its recovery journal has been retained. Close MuseScore and resolve that conflict before exporting again.".into());
    }
    journal.store.write(journal.original.as_deref()).map_err(|e| format!("Could not restore MuseScore's metronome preference; recovery journal retained: {e}"))?;
    if journal.store.read()? != journal.original {
        return Err("MuseScore metronome preference restoration could not be verified; recovery journal retained.".into());
    }
    Ok(())
}

fn remove_journal(path: &Path) -> Result<()> {
    fs::remove_file(path)
        .map_err(|e| format!("Could not remove completed metronome recovery journal: {e}"))?;
    sync_parent(path)
}

impl Store {
    fn discover() -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            Ok(Self::Defaults)
        }
        #[cfg(target_os = "windows")]
        {
            std::env::var_os("APPDATA")
                .map(|p| Self::Ini(PathBuf::from(p).join("MuseScore/MuseScore4.ini")))
                .ok_or_else(|| {
                    "Cannot locate MuseScore release preferences: APPDATA is unavailable.".into()
                })
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config"))).ok_or("Cannot locate MuseScore release preferences: HOME and XDG_CONFIG_HOME are unavailable.")?;
            if !base.is_absolute() {
                return Err(
                    "MuseScore preferences require an absolute XDG_CONFIG_HOME path.".into(),
                );
            }
            Ok(Self::Ini(base.join("MuseScore/MuseScore4.ini")))
        }
    }

    fn read(&self) -> Result<Option<String>> {
        match self {
            Self::Ini(path) => {
                let text = read_ini(path)?;
                let field = ini_field(&text)?;
                field
                    .map(|field| {
                        let value = text[field.value_start..field.value_end].to_owned();
                        validate_bool(&value)?;
                        Ok(value)
                    })
                    .transpose()
            }
            #[cfg(target_os = "macos")]
            Self::Defaults => {
                let output = std::process::Command::new("/usr/bin/defaults")
                    .args(["read", "org.musescore.MuseScore4", KEY])
                    .output()
                    .map_err(|e| format!("Cannot read native MuseScore preferences: {e}"))?;
                if output.status.success() {
                    let kind = std::process::Command::new("/usr/bin/defaults")
                        .args(["read-type", "org.musescore.MuseScore4", KEY])
                        .output()
                        .map_err(|e| e.to_string())?;
                    if !kind.status.success()
                        || !String::from_utf8_lossy(&kind.stdout)
                            .to_ascii_lowercase()
                            .contains("boolean")
                    {
                        return Err("Unsupported MuseScore native metronome preference type; expected a boolean.".into());
                    }
                    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                    validate_bool(&value)?;
                    Ok(Some(value))
                } else if String::from_utf8_lossy(&output.stderr).contains("does not exist") {
                    Ok(None)
                } else {
                    Err(format!(
                        "Cannot read native MuseScore preferences: {}",
                        String::from_utf8_lossy(&output.stderr)
                    ))
                }
            }
        }
    }

    fn write(&self, value: Option<&str>) -> Result<()> {
        if let Some(value) = value {
            validate_bool(value)?;
        }
        match self {
            Self::Ini(path) => {
                let text = read_ini(path)?;
                let edited = edit_ini(&text, value)?;
                atomic_write(path, edited.as_bytes())
            }
            #[cfg(target_os = "macos")]
            Self::Defaults => {
                let mut command = std::process::Command::new("/usr/bin/defaults");
                if let Some(value) = value {
                    command.args([
                        "write",
                        "org.musescore.MuseScore4",
                        KEY,
                        "-bool",
                        if bool_value(value)? { "true" } else { "false" },
                    ]);
                } else {
                    if self.read()?.is_none() {
                        return Ok(());
                    }
                    command.args(["delete", "org.musescore.MuseScore4", KEY]);
                }
                let output = command.output().map_err(|e| {
                    format!("Cannot update native MuseScore metronome preference: {e}")
                })?;
                if output.status.success() {
                    Ok(())
                } else {
                    Err(format!(
                        "Cannot update native MuseScore metronome preference: {}",
                        String::from_utf8_lossy(&output.stderr)
                    ))
                }
            }
        }
    }
}

fn validate_bool(value: &str) -> Result<()> {
    bool_value(value).map(|_| ())
}
fn bool_value(value: &str) -> Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => Err("Unsupported MuseScore native metronome preference value; only boolean release preferences are supported.".into()),
    }
}

fn read_regular_file(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Expected a regular file; symlink preference and recovery files are unsupported",
        ));
    }
    if metadata.len() > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Preference or recovery file exceeds its size limit",
        ));
    }
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Preference or recovery file exceeds its size limit",
        ));
    }
    Ok(bytes)
}

fn read_ini(path: &Path) -> Result<String> {
    match read_regular_file(path, 8 * 1024 * 1024) {
        Ok(bytes) => String::from_utf8(bytes).map_err(|_| {
            "Unsupported MuseScore preference file encoding; expected UTF-8 Qt INI.".into()
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(format!(
            "Cannot read MuseScore preferences {}: {error}",
            path.display()
        )),
    }
}

struct Field {
    line_start: usize,
    line_end: usize,
    value_start: usize,
    value_end: usize,
}

fn ini_field(text: &str) -> Result<Option<Field>> {
    let mut section = "";
    let mut found = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let content = line.trim_end_matches(['\r', '\n']);
        let trimmed = content.trim();
        if trimmed.starts_with('[') {
            if !trimmed.ends_with(']') {
                return Err("Unsupported malformed MuseScore INI section.".into());
            }
            section = &trimmed[1..trimmed.len() - 1];
        } else if !trimmed.starts_with([';', '#']) {
            if let Some(equals) = content.find('=') {
                let key = content[..equals].trim();
                let canonical = (section == "application" && key == "playback\\metronomeEnabled")
                    || (section == "application/playback" && key == "metronomeEnabled")
                    || (section == "General" && key == KEY);
                if canonical {
                    if found.is_some() {
                        return Err("Ambiguous duplicate MuseScore metronome preferences; restore the standard release preference layout.".into());
                    }
                    found = Some(Field {
                        line_start: offset,
                        line_end: offset + line.len(),
                        value_start: offset + equals + 1,
                        value_end: offset + content.len(),
                    });
                } else if key.to_ascii_lowercase().contains("metronomeenabled") {
                    return Err("Unsupported MuseScore metronome preference layout; expected standard MuseScore 4 release preferences.".into());
                }
            }
        }
        offset += line.len();
    }
    Ok(found)
}

fn edit_ini(text: &str, value: Option<&str>) -> Result<String> {
    if let Some(field) = ini_field(text)? {
        let mut output = text.to_owned();
        if let Some(value) = value {
            output.replace_range(field.value_start..field.value_end, value);
        } else {
            output.replace_range(field.line_start..field.line_end, "");
        }
        return Ok(output);
    }
    let Some(value) = value else {
        return Ok(text.to_owned());
    };
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut output = text.to_owned();
    if !output.is_empty() && !output.ends_with('\n') {
        output.push_str(newline);
    }
    // Repeating a section is legal INI, but duplicate groups are avoided for Qt portability.
    let insertion = text
        .split_inclusive('\n')
        .scan(0usize, |offset, line| {
            let start = *offset;
            *offset += line.len();
            Some((start, line))
        })
        .find(|(_, line)| line.trim() == "[application]")
        .map(|(start, line)| start + line.len());
    if let Some(index) = insertion {
        output.insert_str(
            index,
            &format!("playback\\metronomeEnabled={value}{newline}"),
        );
    } else {
        output.push_str(&format!(
            "[application]{newline}playback\\metronomeEnabled={value}{newline}"
        ));
    }
    Ok(output)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or("Invalid preferences path")?;
    fs::create_dir_all(parent).map_err(|e| format!("Cannot create preference directory: {e}"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    if let Ok(metadata) = fs::metadata(path) {
        temporary
            .as_file()
            .set_permissions(metadata.permissions())
            .map_err(|e| e.to_string())?;
    }
    temporary.write_all(bytes).map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    temporary
        .persist(path)
        .map_err(|e| format!("Cannot atomically update {}: {e}", path.display()))?;
    sync_parent(path)
}

fn sync_parent(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path.parent().ok_or("Invalid preference path")?)
        .and_then(|f| f.sync_all())
        .map_err(|e| format!("Cannot sync preference directory: {e}"))?;
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restores_on_off_absent_and_preserves_unrelated_preferences() {
        for original in [Some("true"), Some("false"), None] {
            for requested in [true, false] {
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("preferences.ini");
                let text = format!(
                    "[application]\nother=untouched\n{}[audio]\nvolume=42\n",
                    original
                        .map(|v| format!("playback\\metronomeEnabled={v}\n"))
                        .unwrap_or_default()
                );
                fs::write(&path, text).unwrap();
                let store = Store::Ini(path.clone());
                let mut guard = MetronomePreference::begin_store(
                    &dir.path().join("recovery"),
                    store.clone(),
                    requested,
                )
                .unwrap();
                assert_eq!(
                    store.read().unwrap().as_deref(),
                    Some(if requested { "true" } else { "false" })
                );
                guard.restore().unwrap();
                assert_eq!(store.read().unwrap().as_deref(), original);
                let content = fs::read_to_string(&path).unwrap();
                assert!(content.contains("other=untouched") && content.contains("volume=42"));
                assert!(!dir
                    .path()
                    .join("recovery/musescore-metronome-recovery.json")
                    .exists());
            }
        }
    }
    #[test]
    fn drop_restores_and_lock_rejects_concurrent_exports() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::Ini(dir.path().join("prefs.ini"));
        let guard = MetronomePreference::begin_store(dir.path(), store.clone(), true).unwrap();
        assert!(MetronomePreference::begin_store(dir.path(), store.clone(), false).is_err());
        drop(guard);
        assert_eq!(store.read().unwrap(), None);
    }
    #[test]
    fn recovery_restores_crash_and_conflicts_are_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::Ini(dir.path().join("prefs.ini"));
        store.write(Some("false")).unwrap();
        let guard = MetronomePreference::begin_store(dir.path(), store.clone(), true).unwrap();
        // Model process death without leaking its OS lock into this process.
        let journal = fs::read(&guard.journal_path).unwrap();
        drop(guard);
        store.write(Some("true")).unwrap();
        fs::write(
            dir.path().join("musescore-metronome-recovery.json"),
            &journal,
        )
        .unwrap();
        let mut recovered =
            MetronomePreference::begin_store(dir.path(), store.clone(), true).unwrap();
        recovered.restore().unwrap();
        assert_eq!(store.read().unwrap().as_deref(), Some("false"));
        drop(recovered);
        let mut guard = MetronomePreference::begin_store(dir.path(), store.clone(), true).unwrap();
        store.write(Some("0")).unwrap();
        assert!(guard
            .restore()
            .unwrap_err()
            .contains("changed during rendering"));
        drop(guard);
        assert_eq!(store.read().unwrap().as_deref(), Some("0"));
        assert!(dir
            .path()
            .join("musescore-metronome-recovery.json")
            .exists());
    }
    #[test]
    fn ini_targets_only_one_key_and_preserves_raw_value() {
        let text = "[application]\r\nplayback\\metronomeEnabled=  FALSE \r\nother=1\r\n";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.ini");
        fs::write(&path, text).unwrap();
        let mut guard = MetronomePreference::begin_store(
            &dir.path().join("recovery"),
            Store::Ini(path.clone()),
            true,
        )
        .unwrap();
        guard.restore().unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), text);
        assert!(ini_field(
            "[application]\nplayback\\metronomeEnabled=true\nplayback\\metronomeEnabled=false\n"
        )
        .is_err());
        assert!(edit_ini("[unknown]\nmetronomeEnabled=true\n", Some("false")).is_err());
    }

    #[test]
    fn rejects_oversized_preference_files_before_reading_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large.ini");
        File::create(&path)
            .unwrap()
            .set_len(8 * 1024 * 1024 + 1)
            .unwrap();
        assert!(read_ini(&path).unwrap_err().contains("size limit"));
    }

    #[cfg(unix)]
    #[test]
    fn does_not_replace_a_symlink_preference_store() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("dotfiles.ini");
        let alias = dir.path().join("prefs.ini");
        let original = "[application]\nplayback\\metronomeEnabled=false\n";
        fs::write(&target, original).unwrap();
        std::os::unix::fs::symlink(&target, &alias).unwrap();
        assert!(MetronomePreference::begin_store(
            &dir.path().join("recovery"),
            Store::Ini(alias.clone()),
            true
        )
        .is_err());
        assert!(alias.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(target).unwrap(), original);
    }
}
