use mxml_core::{
    check_output, merge_score, read_score_bytes, split_score, validate_score, write_score, Element,
    EXTENSIONS, MAX_XML_BYTES,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::State;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

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

#[derive(Serialize)]
struct PartSummary {
    id: String,
    name: String,
    instrument_names: Vec<String>,
    voice_count: usize,
}

#[derive(Serialize)]
struct Inspection {
    id: String,
    path: String,
    name: String,
    parts: Vec<PartSummary>,
    error: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
enum Operation {
    Split {
        part_ids: Vec<String>,
        mode: SplitMode,
    },
    Merge {
        groups: Vec<Vec<String>>,
    },
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

fn supported(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
}

fn register_input(path: PathBuf, documents: &SharedDocuments) -> Result<(String, PathBuf), String> {
    if !supported(&path) {
        return Err("Choose a .mxml, .musicxml, .xml, or .mxl score.".into());
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
                PartSummary {
                    id: part.id().into(),
                    name: part.name(),
                    instrument_names,
                    voice_count: voices.len(),
                }
            })
            .collect();
        Ok((bytes, parts))
    })();
    match parsed {
        Ok((bytes, parts)) => {
            lock_documents(documents)?
                .sessions
                .get_mut(&id)
                .ok_or("The score was closed while reading it.")?
                .bytes = Some(Arc::new(bytes));
            result.parts = parts;
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
async fn open_files(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("MusicXML scores", &["mxml", "musicxml", "xml", "mxl"])
            .blocking_pick_files()
            .unwrap_or_default()
            .into_iter()
            .map(|file| {
                file.into_path()
                    .map(|path| path.to_string_lossy().into_owned())
                    .map_err(|error| error.to_string())
            })
            .collect()
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

fn prepare_export(bytes: &[u8], context: &str, operation: Operation) -> Result<Element, String> {
    let score = read_score_bytes(bytes, context).map_err(|error| error.to_string())?;
    let transformed = match operation {
        Operation::Split { part_ids, mode } => split_score(
            &score,
            match mode {
                SplitMode::Chords => "chords",
                SplitMode::Voices => "voices",
            },
            Some(&part_ids),
        ),
        Operation::Merge { groups } => merge_score(&score, &groups),
    }
    .map_err(|error| error.to_string())?;
    validate_score(&transformed).map_err(|error| error.to_string())?;
    Ok(transformed)
}

#[tauri::command]
async fn export_document(
    app: tauri::AppHandle,
    id: String,
    operation: Operation,
    documents: State<'_, SharedDocuments>,
) -> Result<Option<String>, String> {
    let documents = documents.inner().clone();
    let (input, context, bytes) = {
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
        )
    };
    tauri::async_runtime::spawn_blocking(move || {
        let transformed = prepare_export(&bytes, &context, operation)?;
        let stem = Path::new(&context)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        let Some(file) = app
            .dialog()
            .file()
            .set_file_name(format!("{stem}-arranged.musicxml"))
            .add_filter("MusicXML", &["musicxml", "mxml", "xml"])
            .add_filter("Compressed MusicXML", &["mxl"])
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let output = file.into_path().map_err(|error| error.to_string())?;
        if !supported(&output) {
            return Err(
                "The output filename must end with .mxml, .musicxml, .xml, or .mxl.".into(),
            );
        }
        {
            let state = lock_documents(&documents)?;
            protect_sources(&state, &output)?;
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
        protect_sources(&state, &output)?;
        write_score(&transformed, &output, &input, overwrite).map_err(|error| error.to_string())?;
        Ok(Some(output.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|error| error.to_string())?
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(SharedDocuments::default())
        .invoke_handler(tauri::generate_handler![
            inspect_file,
            open_files,
            release_document,
            export_document
        ])
        .run(tauri::generate_context!())
        .expect("Failed to start MXML");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inspect(path: PathBuf, documents: &SharedDocuments) -> Result<Inspection, String> {
        let (id, path) = register_input(path, documents)?;
        inspect_registered(id, path, documents)
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
            part_ids: vec!["P1".into()],
            mode: SplitMode::Chords,
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
                mode: SplitMode::Chords,
            },
        )
        .unwrap_err();
        assert!(error.contains("Unknown part ID"), "{error}");
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
}
