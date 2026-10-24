use mxml_core::{
    check_output, diff_score, merge_score_with_stems, read_score_bytes, split_plan,
    split_score_with_options, validate_score, write_score, Part, SplitFallback, SplitOptions,
    SplitResult, EXTENSIONS, MAX_XML_BYTES,
};
use practice_tracks::{
    ExportReport, PracticeOptions, PracticePart, RendererConfig, RendererEnvironment,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::{Emitter, Manager, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_opener::OpenerExt;

#[derive(Default)]
struct Documents {
    next_id: u64,
    sessions: HashMap<String, Session>,
}

struct Session {
    path: PathBuf,
    context: String,
    bytes: Option<Arc<Vec<u8>>>,
}

type SharedDocuments = Arc<Mutex<Documents>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum DocumentFormat {
    Musicxml,
    Musescore,
}

struct PracticeJob {
    id: String,
    cancelled: Arc<AtomicBool>,
}

type SharedPracticeJob = Arc<Mutex<Option<PracticeJob>>>;

struct PracticeJobGuard(SharedPracticeJob);

impl Drop for PracticeJobGuard {
    fn drop(&mut self) {
        if let Ok(mut job) = self.0.lock() {
            *job = None;
        }
    }
}

#[derive(Serialize)]
struct PartSummary {
    id: String,
    name: String,
    instrument_names: Vec<String>,
    voice_count: usize,
    split_plans: SplitPlans,
}

#[derive(Serialize)]
struct SplitPlans {
    chords: Vec<LaneSummary>,
    voices: Vec<LaneSummary>,
}

#[derive(Serialize)]
struct LaneSummary {
    voice: String,
    tone: usize,
    fallback_voice: Option<String>,
}

fn split_preview(part: &Part, mode: &str) -> Result<Vec<LaneSummary>, String> {
    Ok(split_plan(part, mode)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|lane| LaneSummary {
            voice: lane.voice,
            tone: lane.tone,
            fallback_voice: lane.fallback_voice,
        })
        .collect())
}

#[derive(Serialize)]
struct Inspection {
    id: String,
    path: String,
    name: String,
    parts: Vec<PartSummary>,
    format: DocumentFormat,
    practice_parts: Vec<PracticePart>,
    error: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
enum Operation {
    Split {
        part_ids: Vec<String>,
        mode: SplitMode,
        fallbacks: Vec<FallbackSelection>,
        insert_fallback_slurs: bool,
    },
    Merge {
        groups: Vec<Vec<String>>,
        stems: Vec<Vec<String>>,
    },
    Diff {
        older_id: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FallbackSelection {
    part_id: String,
    voice: String,
    tone: usize,
    fallback_voice: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum SplitMode {
    Chords,
    Voices,
}

fn lock_documents(
    documents: &SharedDocuments,
) -> Result<std::sync::MutexGuard<'_, Documents>, String> {
    documents
        .lock()
        .map_err(|_| "The document store is unavailable. Restart the application.".into())
}

fn supported_musicxml(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
}

fn document_format(path: &Path) -> Option<DocumentFormat> {
    if supported_musicxml(path) {
        Some(DocumentFormat::Musicxml)
    } else if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "mscx" | "mscz"))
    {
        Some(DocumentFormat::Musescore)
    } else {
        None
    }
}

fn supported(path: &Path) -> bool {
    document_format(path).is_some()
}

fn register_input(path: PathBuf, documents: &SharedDocuments) -> Result<(String, PathBuf), String> {
    if !supported(&path) {
        return Err(
            "Choose a MusicXML (.mxml, .musicxml, .xml, .mxl) or MuseScore (.mscz, .mscx) score."
                .into(),
        );
    }
    let context = path.to_string_lossy().into_owned();
    let path = path
        .canonicalize()
        .map_err(|error| format!("Cannot open {}: {error}", path.display()))?;
    let metadata = path.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("The selected path is not a regular file.".into());
    }
    let mut state = lock_documents(documents)?;
    state.next_id += 1;
    let id = format!("document-{}", state.next_id);
    state.sessions.insert(
        id.clone(),
        Session {
            path: path.clone(),
            context,
            bytes: None,
        },
    );
    Ok((id, path))
}

fn inspect_registered(
    id: String,
    path: PathBuf,
    documents: &SharedDocuments,
) -> Result<Inspection, String> {
    let context = lock_documents(documents)?
        .sessions
        .get(&id)
        .ok_or("The score was closed while reading it.")?
        .context
        .clone();
    let mut result = Inspection {
        id: id.clone(),
        path: context.clone(),
        name: Path::new(&context)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        parts: Vec::new(),
        format: document_format(Path::new(&context)).ok_or("Unsupported score format.")?,
        practice_parts: Vec::new(),
        error: None,
    };
    let parsed = (|| -> Result<_, String> {
        let file = File::open(&path)
            .map_err(|error| format!("Cannot open {}: {error}", path.display()))?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        if metadata.len() > MAX_XML_BYTES as u64 {
            return Err("The score exceeds the 64 MiB input limit.".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_XML_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > MAX_XML_BYTES {
            return Err("The score exceeds the 64 MiB input limit.".into());
        }
        if result.format == DocumentFormat::Musescore {
            let archive = practice_tracks::ScoreArchive::read_bytes(&bytes, &context)?;
            return Ok((bytes, Vec::new(), archive.parts()?));
        }
        let score = read_score_bytes(&bytes, &context).map_err(|error| error.to_string())?;
        let parts = score
            .parts
            .iter()
            .map(|part| {
                let voices: HashSet<&str> = part
                    .measures
                    .iter()
                    .flat_map(|measure| &measure.events)
                    .map(|event| event.voice.as_str())
                    .collect();
                let instrument_names = part
                    .definition
                    .children
                    .iter()
                    .filter_map(|child| child.as_element())
                    .filter(|child| child.name == "score-instrument")
                    .filter_map(|instrument| {
                        instrument
                            .get_child("instrument-name")
                            .and_then(|name| name.get_text())
                            .map(|text| text.into_owned())
                    })
                    .collect();
                Ok(PartSummary {
                    id: part.id().into(),
                    name: part.name(),
                    instrument_names,
                    voice_count: voices.len(),
                    split_plans: SplitPlans {
                        chords: split_preview(part, "chords")?,
                        voices: split_preview(part, "voices")?,
                    },
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok((bytes, parts, Vec::new()))
    })();
    match parsed {
        Ok((bytes, parts, practice_parts)) => {
            lock_documents(documents)?
                .sessions
                .get_mut(&id)
                .ok_or("The score was closed while reading it.")?
                .bytes = Some(Arc::new(bytes));
            result.parts = parts;
            result.practice_parts = practice_parts;
        }
        Err(error) => result.error = Some(error),
    }
    Ok(result)
}

#[tauri::command]
async fn inspect_file(
    path: String,
    documents: State<'_, SharedDocuments>,
) -> Result<Inspection, String> {
    let documents = documents.inner().clone();
    // Reserve the source before yielding so exports also protect files still being parsed.
    let (id, path) = register_input(PathBuf::from(path), &documents)?;
    tauri::async_runtime::spawn_blocking(move || inspect_registered(id, path, &documents))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn existing_score_paths(paths: Vec<String>) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        paths
            .into_iter()
            .take(10)
            .filter(|path| Path::new(path).is_file())
            .collect()
    })
    .await
    .map_err(|error| error.to_string())
}

#[tauri::command]
async fn open_file(app: tauri::AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter(
                "Scores",
                &["mxml", "musicxml", "xml", "mxl", "mscz", "mscx"],
            )
            .add_filter("MuseScore scores", &["mscz", "mscx"])
            .add_filter("MusicXML scores", &["mxml", "musicxml", "xml", "mxl"])
            .blocking_pick_file()
            .map(|file| {
                file.into_path()
                    .map(|path| path.to_string_lossy().into_owned())
                    .map_err(|error| error.to_string())
            })
            .transpose()
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn release_document(id: String, documents: State<'_, SharedDocuments>) -> Result<(), String> {
    lock_documents(&documents)?
        .sessions
        .remove(&id)
        .ok_or("Unknown document. It may already be closed.")?;
    Ok(())
}

fn protect_sources(state: &Documents, output: &Path) -> Result<(), String> {
    for session in state.sessions.values() {
        check_output(&session.path, output, true).map_err(|error| error.to_string())?;
        check_output(Path::new(&session.context), output, true)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn prepare_export(
    bytes: &[u8],
    context: &str,
    operation: Operation,
) -> Result<SplitResult, String> {
    let score = read_score_bytes(bytes, context).map_err(|error| error.to_string())?;
    let transformed = match operation {
        Operation::Split {
            part_ids,
            mode,
            fallbacks,
            insert_fallback_slurs,
        } => split_score_with_options(
            &score,
            match mode {
                SplitMode::Chords => "chords",
                SplitMode::Voices => "voices",
            },
            Some(&part_ids),
            SplitOptions {
                insert_fallback_slurs,
                fallbacks: Some(
                    &fallbacks
                        .into_iter()
                        .map(|setting| SplitFallback {
                            part_id: setting.part_id,
                            voice: setting.voice,
                            tone: setting.tone,
                            fallback_voice: setting.fallback_voice,
                        })
                        .collect::<Vec<_>>(),
                ),
            },
        ),
        Operation::Merge { groups, stems } => {
            merge_score_with_stems(&score, &groups, &stems).map(|root| SplitResult {
                root,
                warnings: Vec::new(),
            })
        }
        Operation::Diff { .. } => return Err("Diff requires an older document snapshot.".into()),
    }
    .map_err(|error| error.to_string())?;
    validate_score(&transformed.root).map_err(|error| error.to_string())?;
    Ok(transformed)
}

fn prepare_diff_export(
    newer: &[u8],
    newer_context: &str,
    older: &[u8],
    older_context: &str,
) -> Result<SplitResult, String> {
    let newer = read_score_bytes(newer, newer_context).map_err(|error| error.to_string())?;
    let older = read_score_bytes(older, older_context).map_err(|error| error.to_string())?;
    let result = diff_score(&older, &newer).map_err(|error| error.to_string())?;
    validate_score(&result.root).map_err(|error| error.to_string())?;
    Ok(SplitResult {
        root: result.root,
        warnings: result.warnings,
    })
}

fn diff_snapshot(state: &Documents, operation: &Operation) -> Result<Option<Session>, String> {
    let Operation::Diff { older_id } = operation else {
        return Ok(None);
    };
    let session = state
        .sessions
        .get(older_id)
        .ok_or("Unknown older version. Choose it again.")?;
    if !supported_musicxml(Path::new(&session.context)) {
        return Err("The older version must be a MusicXML score.".into());
    }
    let bytes = session
        .bytes
        .clone()
        .ok_or("The older version could not be parsed and cannot be compared.")?;
    Ok(Some(Session {
        path: session.path.clone(),
        context: session.context.clone(),
        bytes: Some(bytes),
    }))
}

fn protect_export_sources(
    state: &Documents,
    output: &Path,
    sources: &[PathBuf],
) -> Result<(), String> {
    protect_sources(state, output)?;
    for source in sources {
        check_output(source, output, true).map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[derive(Serialize)]
struct ExportResult {
    path: String,
    warnings: Vec<String>,
}

#[tauri::command]
async fn export_document(
    app: tauri::AppHandle,
    id: String,
    operation: Operation,
    documents: State<'_, SharedDocuments>,
) -> Result<Option<ExportResult>, String> {
    let documents = documents.inner().clone();
    let (input, context, bytes, older) = {
        let state = lock_documents(&documents)?;
        let session = state
            .sessions
            .get(&id)
            .ok_or("Unknown document. Open the score again.")?;
        (
            session.path.clone(),
            session.context.clone(),
            session
                .bytes
                .clone()
                .ok_or("This score could not be parsed and cannot be exported.")?,
            diff_snapshot(&state, &operation)?,
        )
    };
    tauri::async_runtime::spawn_blocking(move || {
        let mut sources = vec![input.clone(), PathBuf::from(&context)];
        let suffix = if older.is_some() { "diff" } else { "arranged" };
        let transformed = if let Some(older) = older {
            if !supported_musicxml(Path::new(&context)) {
                return Err("The newer version must be a MusicXML score.".into());
            }
            sources.extend([older.path, PathBuf::from(&older.context)]);
            prepare_diff_export(
                &bytes,
                &context,
                older.bytes.as_ref().unwrap(),
                &older.context,
            )?
        } else {
            prepare_export(&bytes, &context, operation)?
        };
        let stem = Path::new(&context)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        let dialog = app.dialog().file().set_file_name(format!(
            "{stem}-{suffix}.{}",
            if suffix == "diff" { "mxl" } else { "musicxml" }
        ));
        let dialog = if suffix == "diff" {
            dialog
                .add_filter("Compressed MusicXML", &["mxl"])
                .add_filter("MusicXML", &["musicxml", "xml", "mxml"])
        } else {
            dialog
                .add_filter("MusicXML", &["musicxml", "mxml", "xml"])
                .add_filter("Compressed MusicXML", &["mxl"])
        };
        let Some(file) = dialog.blocking_save_file() else {
            return Ok(None);
        };
        let output = file.into_path().map_err(|error| error.to_string())?;
        if !supported_musicxml(&output) {
            return Err(
                "The output filename must end with .mxml, .musicxml, .xml, or .mxl.".into(),
            );
        }
        {
            let state = lock_documents(&documents)?;
            protect_export_sources(&state, &output, &sources)?;
        }
        let overwrite = output.exists();
        if overwrite
            && !app
                .dialog()
                .message(format!(
                    "Replace {}? This cannot be undone.",
                    output.display()
                ))
                .title("Replace existing score?")
                .kind(MessageDialogKind::Warning)
                .buttons(MessageDialogButtons::YesNo)
                .blocking_show()
        {
            return Ok(None);
        }
        // A new queued score must be protected even if opened while this export was preparing.
        let state = lock_documents(&documents)?;
        protect_export_sources(&state, &output, &sources)?;
        write_score(&transformed.root, &output, &input, overwrite)
            .map_err(|error| error.to_string())?;
        Ok(Some(ExportResult {
            path: output.to_string_lossy().into_owned(),
            warnings: transformed.warnings,
        }))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn practice_environment(config: RendererConfig) -> Result<RendererEnvironment, String> {
    tauri::async_runtime::spawn_blocking(move || practice_tracks::environment(config))
        .await
        .map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum PracticePathKind {
    Executable,
    Soundfont,
}

#[tauri::command]
async fn pick_practice_path(
    app: tauri::AppHandle,
    kind: PracticePathKind,
) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dialog = app.dialog().file();
        let dialog = match kind {
            PracticePathKind::Executable => dialog.set_title("Choose MuseScore 4.7 executable"),
            PracticePathKind::Soundfont => dialog
                .set_title("Choose the matching MS Basic SoundFont")
                .add_filter("MS Basic SoundFont", &["sf3"]),
        };
        dialog
            .blocking_pick_file()
            .map(|file| {
                file.into_path()
                    .map(|path| path.to_string_lossy().into_owned())
                    .map_err(|error| error.to_string())
            })
            .transpose()
    })
    .await
    .map_err(|error| error.to_string())?
}

fn begin_practice_job(
    id: String,
    jobs: &SharedPracticeJob,
) -> Result<(Arc<AtomicBool>, PracticeJobGuard), String> {
    let mut state = jobs
        .lock()
        .map_err(|_| "The practice export state is unavailable.")?;
    if state.is_some() {
        return Err("Another practice export is running. Finish or cancel it first.".into());
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    *state = Some(PracticeJob {
        id,
        cancelled: cancelled.clone(),
    });
    Ok((cancelled, PracticeJobGuard(jobs.clone())))
}

fn practice_snapshot(
    id: &str,
    documents: &SharedDocuments,
) -> Result<(String, Arc<Vec<u8>>), String> {
    let state = lock_documents(documents)?;
    let session = state
        .sessions
        .get(id)
        .ok_or("Unknown document. Open the score again.")?;
    if document_format(Path::new(&session.context)) != Some(DocumentFormat::Musescore) {
        return Err("Create Practice Tracks requires a .mscz or .mscx MuseScore score.".into());
    }
    let bytes = session
        .bytes
        .clone()
        .ok_or("This score could not be parsed and cannot be exported.")?;
    Ok((session.context.clone(), bytes))
}

#[derive(Clone, Serialize)]
struct PracticeProgressEvent {
    id: String,
    #[serde(flatten)]
    progress: practice_tracks::Progress,
}

#[tauri::command]
async fn export_practice_tracks(
    app: tauri::AppHandle,
    id: String,
    options: PracticeOptions,
    config: RendererConfig,
    documents: State<'_, SharedDocuments>,
    jobs: State<'_, SharedPracticeJob>,
) -> Result<Option<ExportReport>, String> {
    let (context, bytes) = practice_snapshot(&id, &documents)?;
    let recovery_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|error| error.to_string())?
        .join("practice-tracks");
    let (cancelled, guard) = begin_practice_job(id.clone(), &jobs)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        let Some(folder) = app
            .dialog()
            .file()
            .set_title("Choose where to create the practice tracks folder")
            .blocking_pick_folder()
        else {
            return Ok(None);
        };
        let output = folder.into_path().map_err(|error| error.to_string())?;
        if cancelled.load(Ordering::Relaxed) {
            return Ok(None);
        }
        practice_tracks::export_tracks(
            &bytes,
            &context,
            &options,
            &config,
            &output,
            &recovery_dir,
            &cancelled,
            |progress| {
                let _ = app.emit(
                    "practice-progress",
                    PracticeProgressEvent {
                        id: id.clone(),
                        progress,
                    },
                );
            },
        )
        .map(Some)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn cancel_practice_export(id: String, jobs: State<'_, SharedPracticeJob>) -> Result<(), String> {
    let state = jobs
        .lock()
        .map_err(|_| "The practice export state is unavailable.")?;
    let job = state
        .as_ref()
        .filter(|job| job.id == id)
        .ok_or("No practice export is running for this score.")?;
    job.cancelled.store(true, Ordering::Relaxed);
    Ok(())
}

fn validate_export_directory(directory: &Path) -> Result<(), String> {
    if !directory.is_absolute() || !directory.is_dir() {
        return Err("The exported folder no longer exists or is not an absolute directory.".into());
    }
    Ok(())
}

#[tauri::command]
fn open_practice_export_folder(app: tauri::AppHandle, directory: String) -> Result<(), String> {
    validate_export_directory(Path::new(&directory))?;
    app.opener()
        .open_path(directory, None::<&str>)
        .map_err(|error| error.to_string())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(SharedDocuments::default())
        .manage(SharedPracticeJob::default())
        .invoke_handler(tauri::generate_handler![
            inspect_file,
            existing_score_paths,
            open_file,
            release_document,
            export_document,
            practice_environment,
            pick_practice_path,
            export_practice_tracks,
            cancel_practice_export,
            open_practice_export_folder
        ])
        .run(tauri::generate_context!())
        .expect("Failed to start Voiceweaver");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_export_uses_snapshots_and_protects_both_inputs_after_release() {
        let directory = tempfile::tempdir().unwrap();
        let old_path = directory.path().join("old.musicxml");
        let new_path = directory.path().join("new.musicxml");
        std::fs::write(&old_path, FALLBACK_SCORE).unwrap();
        let newer = String::from_utf8(FALLBACK_SCORE.to_vec())
            .unwrap()
            .replacen("<step>C</step>", "<step>D</step>", 1);
        std::fs::write(&new_path, newer.as_bytes()).unwrap();
        let documents = SharedDocuments::default();
        let old = inspect(old_path.clone(), &documents).unwrap();
        let new = inspect(new_path.clone(), &documents).unwrap();
        let operation = Operation::Diff {
            older_id: old.id.clone(),
        };
        let mut state = lock_documents(&documents).unwrap();
        let snapshot = diff_snapshot(&state, &operation).unwrap().unwrap();
        let new_bytes = state.sessions[&new.id].bytes.clone().unwrap();
        std::fs::write(&old_path, b"changed on disk").unwrap();
        state.sessions.clear();
        let sources = vec![old_path.clone(), new_path.clone()];
        assert!(protect_export_sources(&state, &old_path, &sources).is_err());
        assert!(protect_export_sources(&state, &new_path, &sources).is_err());
        let alias = directory.path().join("old-alias.musicxml");
        std::fs::hard_link(&old_path, &alias).unwrap();
        assert!(protect_export_sources(&state, &alias, &sources).is_err());
        let output = directory.path().join("diff.mxl");
        assert!(protect_export_sources(&state, &output, &sources).is_ok());
        let transformed = prepare_diff_export(
            &new_bytes,
            &new.path,
            snapshot.bytes.as_ref().unwrap(),
            &snapshot.context,
        )
        .unwrap();
        let score = mxml_core::parse_score(transformed.root.clone()).unwrap();
        assert_eq!(
            score.parts[0].measures[0].events[0].notes[0].attributes["color"],
            "#FF0000"
        );
        assert_eq!(
            mxml_core::xml::child_text(
                mxml_core::xml::child(&score.parts[0].measures[0].events[0].notes[0], "pitch")
                    .unwrap(),
                "step"
            )
            .as_deref(),
            Some("D")
        );
        write_score(&transformed.root, &output, &new_path, false).unwrap();
        assert!(std::fs::read(&output).unwrap().starts_with(b"PK\x03\x04"));
        let exported = mxml_core::read_score(&output).unwrap();
        validate_score(&exported.root).unwrap();
        let expected = read_score_bytes(
            &mxml_core::serialize_xml(&transformed.root).unwrap(),
            "expected.musicxml",
        )
        .unwrap();
        assert_eq!(exported.root, expected.root);
        assert_eq!(std::fs::read(&new_path).unwrap(), newer.as_bytes());
        assert!(diff_snapshot(&state, &operation).is_err());
    }

    #[test]
    fn diff_rejects_unparsed_wrong_format_or_missing_older_versions() {
        let mut state = Documents::default();
        let request = Operation::Diff {
            older_id: "older".into(),
        };
        assert!(diff_snapshot(&state, &request).is_err());
        state.sessions.insert(
            "older".into(),
            Session {
                path: "older.musicxml".into(),
                context: "older.musicxml".into(),
                bytes: None,
            },
        );
        assert!(diff_snapshot(&state, &request).is_err());
        state.sessions.get_mut("older").unwrap().context = "older.mscz".into();
        assert!(diff_snapshot(&state, &request)
            .err()
            .unwrap()
            .contains("MusicXML"));
        assert!(prepare_diff_export(FALLBACK_SCORE, "new.xml", b"broken", "old.xml").is_err());
    }

    #[test]
    fn export_folder_requires_an_existing_absolute_directory() {
        let temporary = tempfile::tempdir().unwrap();
        let directory = temporary.path().join("Practice tracks – 音楽 (2)");
        std::fs::create_dir(&directory).unwrap();
        assert!(validate_export_directory(&directory).is_ok());
        assert!(validate_export_directory(&temporary.path().join("missing")).is_err());
        assert!(validate_export_directory(Path::new(".")).is_err());
        assert!(validate_export_directory(Path::new("https://example.com")).is_err());
        let file = directory.join("track.mp3");
        std::fs::write(&file, b"audio").unwrap();
        assert!(validate_export_directory(&file).is_err());
    }

    const FALLBACK_SCORE: &[u8] = concat!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>Choir</part-name></score-part></part-list><part id=\"P1\">",
        "<measure number=\"1\"><attributes><divisions>1</divisions></attributes>",
        "<note><pitch><step>C</step><octave>5</octave></pitch><duration>1</duration><voice>1</voice><type>quarter</type><stem>up</stem></note><backup><duration>1</duration></backup>",
        "<note><pitch><step>E</step><octave>4</octave></pitch><duration>1</duration><voice>2</voice><type>quarter</type><stem>down</stem></note>",
        "<note><chord/><pitch><step>C</step><octave>4</octave></pitch><duration>1</duration><voice>2</voice><type>quarter</type><stem>down</stem></note><backup><duration>1</duration></backup>",
        "<note><pitch><step>G</step><octave>4</octave></pitch><duration>1</duration><voice>3</voice><type>quarter</type><stem>down</stem></note></measure>",
        "<measure number=\"2\"><note><pitch><step>C</step><octave>5</octave></pitch><duration>1</duration><voice>1</voice><type>quarter</type><stem>up</stem></note><backup><duration>1</duration></backup>",
        "<note><pitch><step>E</step><octave>4</octave></pitch><duration>1</duration><voice>2</voice><type>quarter</type><stem>down</stem><lyric><text>alto</text></lyric></note></measure>",
        "<measure number=\"3\"><note><pitch><step>D</step><octave>5</octave></pitch><duration>1</duration><voice>1</voice><type>quarter</type><stem>up</stem></note></measure>",
        "</part></score-partwise>"
    ).as_bytes();

    fn inspect(path: PathBuf, documents: &SharedDocuments) -> Result<Inspection, String> {
        let (id, path) = register_input(path, documents)?;
        inspect_registered(id, path, documents)
    }

    #[test]
    fn recent_scores_include_only_existing_files_in_order_and_limit_checks() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first.xml");
        let second = directory.path().join("second.mscz");
        let deleted = directory.path().join("deleted.xml");
        for path in [&first, &second, &deleted] {
            std::fs::write(path, b"score").unwrap();
        }
        std::fs::remove_file(&deleted).unwrap();
        let paths = [&second, &deleted, directory.path(), &first]
            .map(|path| path.to_string_lossy().into_owned())
            .to_vec();
        assert_eq!(
            tauri::async_runtime::block_on(existing_score_paths(paths)).unwrap(),
            vec![second.to_string_lossy(), first.to_string_lossy()]
        );
        let mut paths = vec![deleted.to_string_lossy().into_owned(); 10];
        paths.push(first.to_string_lossy().into_owned());
        assert!(tauri::async_runtime::block_on(existing_score_paths(paths))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn limits_accepted_extensions() {
        assert!(supported(Path::new("score.MXL")));
        assert!(supported(Path::new("score.musicxml")));
        assert!(!supported(Path::new("score.xml.exe")));
        assert!(!supported(Path::new("score")));
    }

    #[test]
    fn export_preparation_checks_transformation_and_schema() {
        let source = br#"<score-partwise version="4.0"><part-list><score-part id="P1"><part-name>Violin</part-name></score-part></part-list><part id="P1"><measure number="1"><attributes><divisions>1</divisions></attributes><note><pitch><step>C</step><octave>4</octave></pitch><duration>1</duration><voice>2</voice><type>quarter</type></note></measure></part></score-partwise>"#;
        let split = || Operation::Split {
            insert_fallback_slurs: true,
            part_ids: vec!["P1".into()],
            mode: SplitMode::Chords,
            fallbacks: vec![],
        };
        assert!(prepare_export(source, "score.xml", split()).is_ok());
        let invalid = String::from_utf8(source.to_vec())
            .unwrap()
            .replace("<step>C</step>", "<step>H</step>");
        assert!(prepare_export(invalid.as_bytes(), "score.xml", split())
            .unwrap_err()
            .contains("schema validation"));
        let error = prepare_export(
            source,
            "score.xml",
            Operation::Split {
                part_ids: vec!["missing".into()],
                insert_fallback_slurs: true,
                mode: SplitMode::Chords,
                fallbacks: vec![],
            },
        )
        .unwrap_err();
        assert!(error.contains("Unknown part ID"), "{error}");
    }

    #[test]
    fn merge_export_uses_stem_selections_and_removes_same_stem_unisons() {
        let source = br#"<score-partwise version="4.0"><part-list><score-part id="P1"><part-name>S</part-name></score-part><score-part id="P2"><part-name>A</part-name></score-part></part-list><part id="P1"><measure number="1"><attributes><divisions>1</divisions></attributes><note><pitch><step>C</step><octave>4</octave></pitch><duration>1</duration><type>quarter</type></note><note><pitch><step>E</step><octave>4</octave></pitch><duration>1</duration><type>quarter</type></note></measure></part><part id="P2"><measure number="1"><attributes><divisions>1</divisions></attributes><note><pitch><step>D</step><octave>4</octave></pitch><duration>1</duration><type>quarter</type></note><note><pitch><step>E</step><octave>4</octave></pitch><duration>1</duration><type>quarter</type></note></measure></part></score-partwise>"#;
        let request = |stems| Operation::Merge {
            groups: vec![vec!["P1".into(), "P2".into()]],
            stems,
        };
        let result = prepare_export(
            source,
            "score.xml",
            request(vec![vec!["down".into(), "down".into()]]),
        )
        .unwrap();
        let score = mxml_core::parse_score(result.root).unwrap();
        let events = &score.parts[0].measures[0].events;
        assert_eq!(events.len(), 3);
        assert!(events
            .iter()
            .flat_map(|e| &e.notes)
            .all(|n| mxml_core::xml::child_text(n, "stem").as_deref() == Some("down")));
        assert_eq!(
            events
                .iter()
                .find(|e| e.start == mxml_core::frac(1))
                .unwrap()
                .voice,
            "1"
        );
        for stems in [
            vec![],
            vec![vec!["up".into()]],
            vec![vec!["up".into(), "invalid".into()]],
        ] {
            assert!(prepare_export(source, "score.xml", request(stems))
                .unwrap_err()
                .contains("stem direction"));
        }
    }

    #[test]
    fn empire_state_preview_and_export_share_three_chord_lanes() {
        let bytes = include_bytes!(
            "../../../../crates/mxml-core/tests/fixtures/empire-state-of-mind-sopran-alt.mxl"
        );
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("empire-state.mxl");
        std::fs::write(&path, bytes).unwrap();
        let inspected = inspect(path, &SharedDocuments::default()).unwrap();
        assert!(inspected.error.is_none(), "{:?}", inspected.error);
        let part = &inspected.parts[0];
        assert_eq!(part.split_plans.chords.len(), 3);
        assert_eq!(part.split_plans.voices.len(), 3);
        let root = prepare_export(
            bytes,
            "empire-state.mxl",
            Operation::Split {
                part_ids: vec![part.id.clone()],
                mode: SplitMode::Chords,
                insert_fallback_slurs: true,
                fallbacks: part
                    .split_plans
                    .chords
                    .iter()
                    .map(|lane| FallbackSelection {
                        part_id: part.id.clone(),
                        voice: lane.voice.clone(),
                        tone: lane.tone,
                        fallback_voice: lane.fallback_voice.clone(),
                    })
                    .collect(),
            },
        )
        .unwrap()
        .root;
        let score = mxml_core::parse_score(root).unwrap();
        assert_eq!(score.parts.len(), 4);
        for (index, step) in [(4, "G"), (5, "F")] {
            let note = score.parts[3].measures[index]
                .events
                .iter()
                .flat_map(|event| &event.notes)
                .find(|note| note.get_child("pitch").is_some())
                .unwrap();
            assert_eq!(
                mxml_core::xml::child_text(note.get_child("pitch").unwrap(), "step").as_deref(),
                Some(step)
            );
        }
    }

    #[test]
    fn ambiguous_relations_prepare_a_valid_export_with_review_warnings() {
        let source = include_bytes!(
            "../../../../crates/mxml-core/tests/fixtures/ambiguous-relations.musicxml"
        );
        let score = read_score_bytes(source, "choir.musicxml").unwrap();
        let result = prepare_export(
            source,
            "choir.musicxml",
            Operation::Split {
                part_ids: vec!["P1".into()],
                mode: SplitMode::Chords,
                insert_fallback_slurs: true,
                fallbacks: split_plan(&score.parts[0], "chords")
                    .unwrap()
                    .into_iter()
                    .map(|lane| FallbackSelection {
                        part_id: "P1".into(),
                        voice: lane.voice,
                        tone: lane.tone,
                        fallback_voice: None,
                    })
                    .collect(),
            },
        )
        .unwrap();
        assert_eq!(result.warnings.len(), 2);
        assert!(result.warnings.iter().any(|w| w.contains("ambiguous tie")));
        assert!(result.warnings.iter().any(|w| w.contains("ambiguous slur")));
        let exported = mxml_core::parse_score(result.root).unwrap();
        assert_eq!(exported.parts.len(), 3);
        assert!(exported
            .parts
            .iter()
            .flat_map(|p| &p.measures)
            .flat_map(|m| &m.events)
            .flat_map(|e| &e.notes)
            .all(|note| note
                .attributes
                .get("color")
                .is_some_and(|color| color == "#FF0000")));
    }

    #[test]
    fn fallback_slur_warning_reaches_the_gui_export_result() {
        let source = include_bytes!("../../../../crates/mxml-core/tests/fixtures/Test2.mxl");
        let score = read_score_bytes(source, "Test2.mxl").unwrap();
        for insert_fallback_slurs in [true, false] {
            let result = prepare_export(
                source,
                "Test2.mxl",
                Operation::Split {
                    part_ids: vec!["P1".into()],
                    mode: SplitMode::Voices,
                    insert_fallback_slurs,
                    fallbacks: split_plan(&score.parts[0], "voices")
                        .unwrap()
                        .into_iter()
                        .map(|lane| FallbackSelection {
                            part_id: "P1".into(),
                            fallback_voice: (lane.voice != "1").then(|| "1".into()),
                            voice: lane.voice,
                            tone: lane.tone,
                        })
                        .collect(),
                },
            )
            .unwrap();
            assert_eq!(result.warnings.len(), usize::from(insert_fallback_slurs));
            if !insert_fallback_slurs {
                let output = mxml_core::parse_score(result.root).unwrap();
                let note = &output.parts[3].measures[1].events[0];
                assert!(note.notes[0].get_child("rest").is_some());
                assert_eq!(note.duration, score.parts[0].measures[1].events[0].duration);
                continue;
            }
            for detail in [
                "inserted slur",
                "measure 1",
                "voice 3",
                "notes [E4]",
                "measure 2",
                "voice 1",
                "notes [A4]",
            ] {
                assert!(
                    result.warnings[0].contains(detail),
                    "{}",
                    result.warnings[0]
                );
            }
        }
    }

    #[test]
    fn inspection_preview_and_export_use_the_same_fallback_lanes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("choir.xml");
        std::fs::write(&path, FALLBACK_SCORE).unwrap();
        let inspected = inspect(path, &SharedDocuments::default()).unwrap();
        assert!(inspected.error.is_none(), "{:?}", inspected.error);
        let part = &inspected.parts[0];
        assert_eq!(part.split_plans.voices.len(), 3);
        assert_eq!(part.split_plans.chords.len(), 4);
        assert_eq!(part.split_plans.voices[2].voice, "3");
        assert_eq!(
            part.split_plans.voices[2].fallback_voice.as_deref(),
            Some("2")
        );
        assert_eq!(part.split_plans.chords[2].voice, "2");
        assert_eq!(part.split_plans.chords[2].tone, 1);
        let settings = || {
            part.split_plans
                .voices
                .iter()
                .map(|lane| FallbackSelection {
                    part_id: part.id.clone(),
                    voice: lane.voice.clone(),
                    tone: lane.tone,
                    fallback_voice: lane.fallback_voice.clone(),
                })
                .collect::<Vec<_>>()
        };
        let export = |fallbacks| {
            let root = prepare_export(
                FALLBACK_SCORE,
                "choir.xml",
                Operation::Split {
                    part_ids: vec![part.id.clone()],
                    insert_fallback_slurs: true,
                    mode: SplitMode::Voices,
                    fallbacks,
                },
            )
            .unwrap()
            .root;
            mxml_core::model::parse_score(root).unwrap()
        };
        let score = export(settings());
        assert_eq!(score.parts.len(), 4);
        let third = &score.parts[3];
        assert_eq!(
            third.measures[0].events[0].notes[0]
                .get_child("pitch")
                .unwrap()
                .get_child("step")
                .unwrap()
                .get_text()
                .unwrap(),
            "G"
        );
        let fallback = &third.measures[1].events[0].notes[0];
        assert_eq!(
            fallback
                .get_child("pitch")
                .unwrap()
                .get_child("step")
                .unwrap()
                .get_text()
                .unwrap(),
            "E"
        );
        assert_eq!(
            fallback
                .get_child("lyric")
                .unwrap()
                .get_child("text")
                .unwrap()
                .get_text()
                .unwrap(),
            "alto"
        );
        assert!(third.measures[2].events[0].notes[0]
            .get_child("rest")
            .is_some());
        let original = read_score_bytes(FALLBACK_SCORE, "choir.xml").unwrap();
        assert_eq!(score.parts[0].source, original.parts[0].source);

        let mut silent = settings();
        silent[2].fallback_voice = None;
        assert!(export(silent).parts[3].measures[1].events[0].notes[0]
            .get_child("rest")
            .is_some());
        let mut override_voice = settings();
        override_voice[2].fallback_voice = Some("1".into());
        assert_eq!(
            export(override_voice).parts[3].measures[1].events[0].notes[0]
                .get_child("pitch")
                .unwrap()
                .get_child("step")
                .unwrap()
                .get_text()
                .unwrap(),
            "C"
        );
    }

    #[test]
    fn export_rejects_missing_and_forged_fallback_settings() {
        let request = |fallbacks| Operation::Split {
            insert_fallback_slurs: true,
            part_ids: vec!["P1".into()],
            mode: SplitMode::Voices,
            fallbacks,
        };
        assert!(prepare_export(FALLBACK_SCORE, "choir.xml", request(vec![])).is_err());
        assert!(prepare_export(
            FALLBACK_SCORE,
            "choir.xml",
            request(vec![FallbackSelection {
                part_id: "missing".into(),
                voice: "3".into(),
                tone: 0,
                fallback_voice: Some("2".into()),
            }])
        )
        .is_err());
    }

    #[test]
    fn malformed_queued_scores_remain_protected_from_export() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("broken.xml");
        std::fs::write(&path, b"<not-a-score/>").unwrap();
        let documents = SharedDocuments::default();
        let inspected = inspect(path.clone(), &documents).unwrap();
        assert!(inspected.error.is_some());
        let state = lock_documents(&documents).unwrap();
        assert!(state.sessions[&inspected.id].bytes.is_none());
        assert!(protect_sources(&state, &path).is_err());
        let alias = directory.path().join("alias.xml");
        std::fs::hard_link(&path, &alias).unwrap();
        assert!(protect_sources(&state, &alias).is_err());
        assert!(protect_sources(&state, &directory.path().join("new.xml")).is_ok());
    }

    #[test]
    fn pending_inspections_protect_the_source_before_parsing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("score.xml");
        std::fs::write(&path, b"not parsed yet").unwrap();
        let documents = SharedDocuments::default();
        register_input(path.clone(), &documents).unwrap();
        assert!(protect_sources(&lock_documents(&documents).unwrap(), &path).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_keeps_the_selected_archive_extension() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("plain.xml");
        let alias = directory.path().join("archive.mxl");
        std::fs::write(&path, b"<not-a-score/>").unwrap();
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        let inspected = inspect(alias, &SharedDocuments::default()).unwrap();
        assert_eq!(inspected.name, "archive.mxl");
        assert!(inspected
            .error
            .unwrap()
            .contains("compressed MusicXML archive"));
    }

    #[test]
    fn inspection_exposes_stable_ids_and_keeps_immutable_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("score.xml");
        let source = br#"<score-partwise version="4.0"><part-list><score-part id="P1"><part-name>Violin</part-name><score-instrument id="I1"><instrument-name>Violin</instrument-name></score-instrument></score-part></part-list><part id="P1"><measure number="1"><attributes><divisions>1</divisions></attributes><note><rest/><duration>4</duration><voice>2</voice><type>whole</type></note></measure></part></score-partwise>"#;
        std::fs::write(&path, source).unwrap();
        let documents = SharedDocuments::default();
        let inspected = inspect(path.clone(), &documents).unwrap();
        assert!(inspected.error.is_none(), "{:?}", inspected.error);
        assert_eq!(inspected.parts[0].id, "P1");
        assert_eq!(inspected.parts[0].instrument_names, ["Violin"]);
        assert_eq!(inspected.parts[0].voice_count, 1);
        std::fs::write(&path, b"changed outside the app").unwrap();
        assert_eq!(
            lock_documents(&documents).unwrap().sessions[&inspected.id]
                .bytes
                .as_ref()
                .unwrap()
                .as_slice(),
            source
        );
    }

    #[test]
    fn musescore_inspection_uses_an_immutable_snapshot_and_all_parts() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("practice.mscx");
        let source = br#"<museScore version="4.70"><Score><Part id="1"><trackName>Choir</trackName><Staff id="1"/><Instrument id="voice"><Channel><program value="0"/></Channel></Instrument></Part><Part id="2"><trackName>Choir</trackName><show>0</show><Staff id="2"/><Instrument id="voice"/></Part><Staff id="1"><Measure/></Staff><Staff id="2"><Measure/></Staff></Score></museScore>"#;
        std::fs::write(&path, source).unwrap();
        let documents = SharedDocuments::default();
        let inspected = inspect(path.clone(), &documents).unwrap();
        assert!(inspected.error.is_none(), "{:?}", inspected.error);
        assert_eq!(inspected.format, DocumentFormat::Musescore);
        assert!(inspected.parts.is_empty());
        assert_eq!(inspected.practice_parts.len(), 2);
        assert_ne!(
            inspected.practice_parts[0].key,
            inspected.practice_parts[1].key
        );
        assert!(!inspected.practice_parts[1].visible);
        std::fs::write(&path, b"changed outside VoiceWeaver").unwrap();
        let (_, snapshot) = practice_snapshot(&inspected.id, &documents).unwrap();
        assert_eq!(snapshot.as_slice(), source);
    }

    #[test]
    fn practice_export_rejects_wrong_formats_and_failed_inspections() {
        let directory = tempfile::tempdir().unwrap();
        let documents = SharedDocuments::default();
        let path = directory.path().join("broken.mscx");
        std::fs::write(&path, b"invalid XML").unwrap();
        let inspected = inspect(path, &documents).unwrap();
        assert!(inspected.error.is_some());
        assert!(practice_snapshot(&inspected.id, &documents).is_err());
        let path = directory.path().join("music.xml");
        std::fs::write(&path, FALLBACK_SCORE).unwrap();
        let inspected = inspect(path, &documents).unwrap();
        assert!(practice_snapshot(&inspected.id, &documents)
            .unwrap_err()
            .contains("requires"));
        assert!(supported(Path::new("score.MSCZ")));
        assert!(supported(Path::new("score.mscx")));
        assert!(!supported_musicxml(Path::new("score.mscz")));
    }

    #[test]
    fn only_one_practice_job_runs_and_its_guard_releases_the_slot() {
        let jobs = SharedPracticeJob::default();
        let (cancelled, guard) = begin_practice_job("document-1".into(), &jobs).unwrap();
        assert!(begin_practice_job("document-2".into(), &jobs).is_err());
        jobs.lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .cancelled
            .store(true, Ordering::Relaxed);
        assert!(cancelled.load(Ordering::Relaxed));
        drop(guard);
        assert!(begin_practice_job("document-2".into(), &jobs).is_ok());
    }
}
