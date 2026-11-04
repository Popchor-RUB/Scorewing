//! Plans and publishes direct exports without silently replacing existing files.
use crate::Result;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs::{self, Metadata, OpenOptions};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OutputChoice {
    Create,
    Overwrite,
    Clear,
    Cancel,
}

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    length: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    #[cfg(unix)]
    identity: (u64, u64),
}

impl Snapshot {
    fn of(metadata: &Metadata) -> Self {
        Self {
            length: metadata.len(),
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
            #[cfg(unix)]
            identity: {
                use std::os::unix::fs::MetadataExt;
                (metadata.dev(), metadata.ino())
            },
        }
    }

    fn same_directory(&self, other: &Self) -> bool {
        // Directory contents change during staging; compare the directory itself.
        self.created == other.created && {
            #[cfg(unix)]
            {
                self.identity == other.identity
            }
            #[cfg(not(unix))]
            {
                true
            }
        }
    }
}

fn snapshot(path: &Path, directory: bool) -> Result<Option<Snapshot>> {
    match fs::symlink_metadata(path) {
        Ok(metadata)
            if !metadata.file_type().is_symlink()
                && if directory {
                    metadata.is_dir()
                } else {
                    metadata.is_file()
                } =>
        {
            Ok(Some(Snapshot::of(&metadata)))
        }
        Ok(_) => Err(format!(
            "Unexpected file type at {}. Choose another output folder.",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("Cannot inspect {}: {error}", path.display())),
    }
}

pub struct OutputPlan {
    directory: PathBuf,
    root: Snapshot,
    folders: BTreeMap<PathBuf, Option<Snapshot>>,
    files: BTreeMap<PathBuf, Option<Snapshot>>,
    protected_sources: Vec<PathBuf>,
}

impl OutputPlan {
    pub(crate) fn new(
        directory: &Path,
        paths: Vec<PathBuf>,
        protected_sources: Vec<PathBuf>,
    ) -> Result<Self> {
        let directory = directory
            .canonicalize()
            .map_err(|e| format!("Cannot open output folder: {e}"))?;
        let root = snapshot(&directory, true)?.ok_or("Select an existing output folder.")?;
        let mut folders = BTreeMap::new();
        let mut files = BTreeMap::new();
        for path in paths {
            if path.components().count() != 2
                || path
                    .components()
                    .any(|c| !matches!(c, Component::Normal(_)))
            {
                return Err("Invalid practice track output path.".into());
            }
            let path = directory.join(path);
            let parent = path.parent().unwrap().to_path_buf();
            if !folders.contains_key(&parent) {
                folders.insert(parent.clone(), snapshot(&parent, true)?);
            }
            files.insert(path.clone(), snapshot(&path, false)?);
        }
        if files.is_empty() {
            return Err("Choose at least one instrument and range to export.".into());
        }
        Ok(Self {
            directory,
            root,
            folders,
            files,
            protected_sources,
        })
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn total(&self) -> usize {
        self.files.len()
    }
    pub fn conflicts(&self) -> usize {
        self.files.values().filter(|state| state.is_some()).count()
    }

    pub fn protect_sources(&self, choice: OutputChoice, sources: &[PathBuf]) -> Result<()> {
        for source in self.protected_sources.iter().chain(sources) {
            for (output, state) in &self.files {
                if state.is_some() {
                    scorewing_core::check_source_replacement(source, output)
                        .map_err(|e| e.to_string())?;
                }
            }
            if choice == OutputChoice::Clear {
                let location = source
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new("."))
                    .canonicalize()
                    .map(|parent| parent.join(source.file_name().unwrap_or_default()))
                    .map_err(|e| {
                        format!(
                            "Cannot verify source protection for {}: {e}",
                            source.display()
                        )
                    })?;
                let resolved = source.canonicalize().unwrap_or_else(|_| location.clone());
                if location.starts_with(&self.directory) || resolved.starts_with(&self.directory) {
                    return Err("Cannot clear this folder because it contains a source score or renderer file. Choose another output folder or overwrite matching files.".into());
                }
            }
        }
        Ok(())
    }

    fn check_root(&self) -> Result<()> {
        let current = snapshot(&self.directory, true)?.ok_or("The output folder disappeared.")?;
        if !self.root.same_directory(&current) {
            return Err(
                "The output folder changed. Export again to review the destination.".into(),
            );
        }
        Ok(())
    }

    fn check_folder(&self, path: &Path, expected: &Option<Snapshot>) -> Result<()> {
        let current = snapshot(path, true)?;
        if let Some(expected) = expected {
            if !current
                .as_ref()
                .is_some_and(|current| expected.same_directory(current))
            {
                return Err(
                    "A section output folder changed. Export again to review the destination."
                        .into(),
                );
            }
        }
        Ok(())
    }

    pub fn prepare(mut self, choice: OutputChoice) -> Result<PreparedOutput> {
        if choice == OutputChoice::Cancel {
            return Err("Export cancelled.".into());
        }
        self.protect_sources(choice, &[])?;
        self.check_root()?;
        for (folder, state) in &self.folders {
            self.check_folder(folder, state)?;
        }
        for (path, state) in &self.files {
            if &snapshot(path, false)? != state {
                return Err("An output file changed since confirmation. Export again to review the destination.".into());
            }
        }
        if choice == OutputChoice::Create && self.conflicts() > 0 {
            return Err("Existing tracks require confirmation before overwriting.".into());
        }
        if choice == OutputChoice::Clear {
            if self.directory.parent().is_none() {
                return Err("Cannot clear a filesystem root.".into());
            }
            for entry in fs::read_dir(&self.directory)
                .map_err(|e| format!("Cannot read output folder: {e}"))?
            {
                let entry = entry.map_err(|e| e.to_string())?;
                let result = if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                    fs::remove_dir_all(entry.path())
                } else {
                    fs::remove_file(entry.path())
                };
                result.map_err(|e| format!("Could not finish clearing {}: {e}. Some contents may already have been removed.", self.directory.display()))?;
            }
            self.files.values_mut().for_each(|state| *state = None);
            self.folders.values_mut().for_each(|state| *state = None);
        }
        Ok(PreparedOutput {
            plan: self,
            overwrite: choice == OutputChoice::Overwrite,
        })
    }
}

pub struct PreparedOutput {
    plan: OutputPlan,
    overwrite: bool,
}

impl PreparedOutput {
    pub fn directory(&self) -> &Path {
        self.plan.directory()
    }

    pub(crate) fn publish(&self, source: &Path, destination: &Path) -> Result<()> {
        self.plan.check_root()?;
        let expected = self
            .plan
            .files
            .get(destination)
            .ok_or("Unplanned practice track output.")?;
        let folder = destination.parent().unwrap();
        self.plan.check_folder(folder, &self.plan.folders[folder])?;
        if &snapshot(destination, false)? != expected {
            return Err("An output file changed while rendering. Existing files were preserved; export again to confirm.".into());
        }
        fs::create_dir_all(folder)
            .map_err(|e| format!("Cannot create section output folder: {e}"))?;
        publish(source, destination, self.overwrite && expected.is_some())
    }
}

fn publish(source: &Path, destination: &Path, overwrite: bool) -> Result<()> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(source)
        .and_then(|f| f.sync_all())
        .map_err(|e| format!("Cannot sync completed MP3: {e}"))?;
    let temporary = tempfile::TempPath::try_from_path(source)
        .map_err(|e| format!("Cannot prepare completed MP3 publication: {e}"))?;
    // Both forms publish the completed file atomically; new files never clobber a late arrival.
    let result = if overwrite {
        temporary.persist(destination)
    } else {
        temporary.persist_noclobber(destination)
    };
    result.map_err(|error| {
        let message = format!("Cannot publish {}: {}", destination.display(), error.error);
        let _ = error.path.keep();
        message
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(directory: &Path) -> OutputPlan {
        OutputPlan::new(
            directory,
            vec![PathBuf::from("01 - Intro/01 - Piano.mp3")],
            vec![],
        )
        .unwrap()
    }

    #[test]
    fn overwrite_preserves_other_contents_and_publishes_directly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("01 - Intro/01 - Piano.mp3");
        fs::create_dir(path.parent().unwrap()).unwrap();
        fs::write(&path, b"old").unwrap();
        fs::write(dir.path().join("notes"), b"keep").unwrap();
        let pending = plan(dir.path());
        assert_eq!(pending.conflicts(), 1);
        assert!(plan(dir.path()).prepare(OutputChoice::Create).is_err());
        let output = pending.prepare(OutputChoice::Overwrite).unwrap();
        let path = output.directory().join("01 - Intro/01 - Piano.mp3");
        let source = dir.path().join("completed");
        fs::write(&source, b"audio").unwrap();
        output.publish(&source, &path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"audio");
        assert_eq!(fs::read(dir.path().join("notes")).unwrap(), b"keep");
        assert!(!source.exists());
    }

    #[test]
    fn cancel_is_read_only_and_clear_removes_all_contents() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("unrelated/nested")).unwrap();
        fs::write(dir.path().join("unrelated/nested/keep"), b"data").unwrap();
        assert!(plan(dir.path()).prepare(OutputChoice::Cancel).is_err());
        assert!(dir.path().join("unrelated/nested/keep").is_file());
        let output = plan(dir.path()).prepare(OutputChoice::Clear).unwrap();
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
        let source = dir.path().join("completed");
        fs::write(&source, b"audio").unwrap();
        output
            .publish(
                &source,
                &output.directory().join("01 - Intro/01 - Piano.mp3"),
            )
            .unwrap();
    }

    #[test]
    fn changed_destinations_and_type_mismatches_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("01 - Intro/01 - Piano.mp3");
        let pending = plan(dir.path());
        fs::create_dir(path.parent().unwrap()).unwrap();
        fs::write(&path, b"arrived").unwrap();
        assert!(pending.prepare(OutputChoice::Overwrite).is_err());
        let output = plan(dir.path()).prepare(OutputChoice::Overwrite).unwrap();
        let path = output.directory().join("01 - Intro/01 - Piano.mp3");
        fs::write(&path, b"changed after confirmation").unwrap();
        let source = dir.path().join("completed");
        fs::write(&source, b"audio").unwrap();
        assert!(output.publish(&source, &path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"changed after confirmation");
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(OutputPlan::new(
            dir.path(),
            vec![PathBuf::from("01 - Intro/01 - Piano.mp3")],
            vec![]
        )
        .is_err());
    }

    #[test]
    fn clear_protects_sources_in_the_destination() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("score.mscz");
        fs::write(&source, b"score").unwrap();
        assert!(plan(dir.path())
            .protect_sources(OutputChoice::Clear, std::slice::from_ref(&source))
            .is_err());
        plan(dir.path())
            .protect_sources(OutputChoice::Overwrite, &[source])
            .unwrap();
    }

    #[test]
    fn late_arrivals_never_clobber_files_or_trigger_a_clear() {
        let dir = tempfile::tempdir().unwrap();
        let output = plan(dir.path()).prepare(OutputChoice::Create).unwrap();
        let destination = output.directory().join("01 - Intro/01 - Piano.mp3");
        let pending_clear = plan(dir.path());
        fs::create_dir(destination.parent().unwrap()).unwrap();
        fs::write(&destination, b"late arrival").unwrap();
        let source = dir.path().join("completed");
        fs::write(&source, b"audio").unwrap();
        assert!(output.publish(&source, &destination).is_err());
        assert!(pending_clear.prepare(OutputChoice::Clear).is_err());
        assert_eq!(fs::read(&source).unwrap(), b"audio");
        assert_eq!(fs::read(&destination).unwrap(), b"late arrival");
    }

    #[test]
    fn renderer_files_are_protected_even_without_caller_supplied_sources() {
        let dir = tempfile::tempdir().unwrap();
        let renderer = dir.path().join("renderer");
        fs::write(&renderer, b"executable").unwrap();
        let output = OutputPlan::new(
            dir.path(),
            vec![PathBuf::from("01 - Intro/01 - Piano.mp3")],
            vec![renderer.clone()],
        )
        .unwrap();
        assert!(output.prepare(OutputChoice::Clear).is_err());
        assert_eq!(fs::read(renderer).unwrap(), b"executable");
    }

    #[test]
    #[cfg(unix)]
    fn symlink_sections_are_rejected_and_clear_does_not_follow_other_links() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("keep"), b"safe").unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("01 - Intro")).unwrap();
        assert!(OutputPlan::new(
            dir.path(),
            vec![PathBuf::from("01 - Intro/01 - Piano.mp3")],
            vec![]
        )
        .is_err());
        fs::remove_file(dir.path().join("01 - Intro")).unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("unrelated-link")).unwrap();
        plan(dir.path()).prepare(OutputChoice::Clear).unwrap();
        assert_eq!(fs::read(outside.path().join("keep")).unwrap(), b"safe");
        let source_link = dir.path().join("source.mscz");
        std::os::unix::fs::symlink(outside.path().join("keep"), &source_link).unwrap();
        assert!(plan(dir.path())
            .protect_sources(OutputChoice::Clear, &[source_link])
            .is_err());
    }
}
