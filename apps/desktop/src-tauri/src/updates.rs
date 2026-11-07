use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    io::{Read, Write},
    sync::Mutex,
    time::Duration,
};
use tauri::{ipc::Channel, Manager, State, WebviewWindow};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::UpdaterExt;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReleaseChannel {
    Stable,
    Nightly,
}

impl ReleaseChannel {
    fn endpoint(self) -> &'static str {
        match self {
            Self::Stable => {
                "https://github.com/Popchor-RUB/Scorewing/releases/latest/download/latest.json"
            }
            Self::Nightly => {
                "https://github.com/Popchor-RUB/Scorewing/releases/download/nightly/latest.json"
            }
        }
    }

    fn release_page(self) -> &'static str {
        match self {
            Self::Stable => "https://github.com/Popchor-RUB/Scorewing/releases/latest",
            Self::Nightly => "https://github.com/Popchor-RUB/Scorewing/releases/tag/nightly",
        }
    }
}

pub fn require_main(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("Only the main window can manage updates.".into())
    }
}

fn supported(app: &tauri::AppHandle) -> bool {
    #[cfg(target_os = "linux")]
    {
        app.env().appimage.is_some()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = app;
        true
    }
}

fn require_supported(app: &tauri::AppHandle) -> Result<(), String> {
    if supported(app) {
        Ok(())
    } else {
        Err("In-app updates require launching the Linux AppImage.".into())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEnvironment {
    current_version: String,
    supported: bool,
    startup_error: Option<String>,
}

#[tauri::command]
pub fn update_environment(
    window: WebviewWindow,
    state: State<'_, UpdateState>,
) -> Result<UpdateEnvironment, String> {
    require_main(&window)?;
    Ok(UpdateEnvironment {
        current_version: window.package_info().version.to_string(),
        supported: supported(window.app_handle()),
        startup_error: state
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .startup_error
            .clone(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMetadata {
    rid: tauri::ResourceId,
    current_version: String,
    version: String,
    body: Option<String>,
    raw_json: serde_json::Value,
    downgrade: bool,
}

fn should_offer(order: Ordering, manual: bool) -> bool {
    order == Ordering::Greater || (manual && order == Ordering::Less)
}

#[tauri::command]
pub async fn check_update(
    window: WebviewWindow,
    channel: ReleaseChannel,
    manual: bool,
) -> Result<Option<UpdateMetadata>, String> {
    require_main(&window)?;
    let updater = window
        .updater_builder()
        .endpoints(vec![channel
            .endpoint()
            .parse()
            .map_err(|error| format!("{error}"))?])
        .map_err(|error| error.to_string())?
        .timeout(Duration::from_secs(15))
        .version_comparator(move |current, remote| {
            should_offer(remote.version.cmp(&current), manual)
        })
        .build()
        .map_err(|error| error.to_string())?;
    let Some(update) = updater.check().await.map_err(|error| error.to_string())? else {
        return Ok(None);
    };
    let downgrade = window.package_info().version.cmp(
        &update
            .version
            .parse()
            .map_err(|error| format!("Invalid update version: {error}"))?,
    ) == Ordering::Greater;
    let metadata = UpdateMetadata {
        current_version: update.current_version.clone(),
        version: update.version.clone(),
        body: update.body.clone(),
        raw_json: update.raw_json.clone(),
        // Reuse the plugin resource so its signed download/install API remains authoritative.
        rid: window.resources_table().add(update),
        downgrade,
    };
    Ok(Some(metadata))
}

#[tauri::command]
pub fn open_update_downloads(window: WebviewWindow, channel: ReleaseChannel) -> Result<(), String> {
    require_main(&window)?;
    window
        .opener()
        .open_url(channel.release_page(), None::<&str>)
        .map_err(|error| error.to_string())
}

const MAX_PACKAGE: u64 = 512 * 1024 * 1024;
const MAX_METADATA: u64 = 1024 * 1024;
const STAGED_FILE: &str = "scheduled-update";

#[derive(Default)]
pub struct UpdateState(Mutex<UpdateSession>);

#[derive(Default)]
struct UpdateSession {
    owner: Option<std::fs::File>,
    transfer: Option<tokio::sync::oneshot::Sender<()>>,
    generation: u64,
    ready: Option<(tauri_plugin_updater::Update, Vec<u8>)>,
    installing: bool,
    staged: bool,
    startup_error: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum DownloadEvent {
    #[serde(rename_all = "camelCase")]
    Started {
        content_length: Option<u64>,
    },
    #[serde(rename_all = "camelCase")]
    Progress {
        chunk_length: usize,
    },
    Finished,
}

fn staged_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join(STAGED_FILE))
}

fn acquire_owner(path: &std::path::Path) -> Result<std::fs::File, String> {
    std::fs::create_dir_all(path.parent().ok_or("Invalid update directory.")?)
        .map_err(|error| error.to_string())?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_extension("lock"))
        .map_err(|error| error.to_string())?;
    file.try_lock().map_err(|error| {
        format!("Another Scorewing instance may be managing updates. Close it and restart: {error}")
    })?;
    Ok(file)
}

fn require_owner(session: &UpdateSession) -> Result<(), String> {
    if session.owner.is_none() {
        return Err("Another Scorewing instance owns the updater. Close it and restart.".into());
    }
    Ok(())
}

// A single atomically renamed file makes incomplete downloads impossible to restore.
fn save_staged(
    path: &std::path::Path,
    metadata: &serde_json::Value,
    bytes: &[u8],
) -> Result<(), String> {
    let metadata = serde_json::to_vec(metadata).map_err(|e| e.to_string())?;
    if metadata.is_empty()
        || metadata.len() as u64 > MAX_METADATA
        || bytes.is_empty()
        || bytes.len() as u64 > MAX_PACKAGE
    {
        return Err("Update package or metadata has an invalid size.".into());
    }
    let dir = path.parent().ok_or("Invalid update directory.")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
    file.write_all(&(metadata.len() as u64).to_le_bytes())
        .and_then(|_| file.write_all(&metadata))
        .and_then(|_| file.write_all(bytes))
        .and_then(|_| file.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    std::fs::File::open(dir)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn load_staged(path: &std::path::Path) -> Result<(serde_json::Value, Vec<u8>), String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    if size <= 8 || size > 8 + MAX_METADATA + MAX_PACKAGE {
        return Err("Invalid scheduled update size.".into());
    }
    let mut length = [0; 8];
    file.read_exact(&mut length).map_err(|e| e.to_string())?;
    let length = u64::from_le_bytes(length);
    if length == 0 || length > MAX_METADATA || length >= size - 8 || size - 8 - length > MAX_PACKAGE
    {
        return Err("Invalid scheduled update metadata.".into());
    }
    let mut metadata = vec![0; length as usize];
    file.read_exact(&mut metadata).map_err(|e| e.to_string())?;
    let metadata = serde_json::from_slice(&metadata)
        .map_err(|e| format!("Invalid scheduled update metadata: {e}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_PACKAGE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_PACKAGE {
        return Err("Invalid scheduled update package.".into());
    }
    Ok((metadata, bytes))
}

fn remove_staged(path: &std::path::Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    #[cfg(unix)]
    std::fs::File::open(path.parent().ok_or("Invalid update directory.")?)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn restore(
    app: &tauri::AppHandle,
    metadata: serde_json::Value,
) -> Result<tauri_plugin_updater::Update, String> {
    app.updater_builder()
        .build()
        .map_err(|e| e.to_string())?
        .restore_update(metadata)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "The scheduled update is no longer newer than this application.".into())
}

pub fn set_startup_error(state: &UpdateState, error: String) {
    if let Ok(mut session) = state.0.lock() {
        session.startup_error = Some(error);
    }
}

pub fn startup(app: &tauri::AppHandle) -> Result<bool, String> {
    let path = staged_path(app)?;
    let owner = acquire_owner(&path)?;
    app.state::<UpdateState>()
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .owner = Some(owner);
    if !path.exists() {
        return Ok(false);
    }
    let result = (|| {
        require_supported(app)?;
        let (metadata, bytes) = load_staged(&path)?;
        let update = restore(app, metadata)?;
        update.verify(&bytes).map_err(|e| e.to_string())?;
        // Windows may exit inside install, so consume the schedule first.
        remove_staged(&path)?;
        update.install(&bytes).map_err(|e| e.to_string())?;
        Ok(true)
    })();
    if result.is_err() {
        let _ = remove_staged(&path);
    } else {
        // The replacement process must acquire ownership before this process exits.
        app.state::<UpdateState>()
            .0
            .lock()
            .map_err(|error| error.to_string())?
            .owner = None;
    }
    result
}

#[tauri::command]
pub async fn download_update(
    window: WebviewWindow,
    rid: tauri::ResourceId,
    deferred: bool,
    on_event: Channel<DownloadEvent>,
    state: State<'_, UpdateState>,
) -> Result<(), String> {
    require_main(&window)?;
    require_supported(window.app_handle())?;
    let mut update = (*window
        .resources_table()
        .get::<tauri_plugin_updater::Update>(rid)
        .map_err(|e| e.to_string())?)
    .clone();
    update.timeout = Some(Duration::from_secs(10 * 60));
    if deferred {
        // Restore checks current platform, newer version and signed version before publication.
        let restored = restore(window.app_handle(), update.raw_json.clone())?;
        if restored.signature != update.signature || restored.version != update.version {
            return Err("Update metadata does not match the selected release.".into());
        }
    }
    let (cancel, cancelled) = tokio::sync::oneshot::channel();
    let generation = {
        let mut session = state.0.lock().map_err(|e| e.to_string())?;
        require_owner(&session)?;
        if session.transfer.is_some() || session.installing || session.staged {
            return Err("An update operation is already in progress.".into());
        }
        session.ready = None;
        session.transfer = Some(cancel);
        session.generation = session.generation.wrapping_add(1);
        session.generation
    };
    let mut first = true;
    let download = update.download(
        |chunk_length, content_length| {
            let Ok(session) = state.0.lock() else {
                return;
            };
            if session.generation != generation || session.transfer.is_none() {
                return;
            }
            if first {
                first = false;
                let _ = on_event.send(DownloadEvent::Started { content_length });
            }
            let _ = on_event.send(DownloadEvent::Progress { chunk_length });
        },
        || {},
    );
    let result = tokio::select! {
        biased;
        _ = cancelled => Err("Update download cancelled.".to_string()),
        result = download => result.map_err(|e| e.to_string()),
    };
    let mut session = state.0.lock().map_err(|e| e.to_string())?;
    if session.generation != generation || session.transfer.is_none() {
        return Err("Update download cancelled.".into());
    }
    session.transfer.take();
    let bytes = result?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_PACKAGE {
        return Err("Update package has an invalid size.".into());
    }
    if deferred {
        let restored = restore(window.app_handle(), update.raw_json.clone())?;
        restored.verify(&bytes).map_err(|e| e.to_string())?;
        let path = staged_path(window.app_handle())?;
        if let Err(error) = save_staged(&path, &update.raw_json, &bytes) {
            if path.exists() {
                session.staged = true;
                remove_staged(&path).map_err(|cleanup| {
                    format!("{error}; cannot remove scheduled update: {cleanup}")
                })?;
                session.staged = false;
            }
            return Err(error);
        }
        session.staged = true;
    } else {
        session.ready = Some((update, bytes));
    }
    let _ = on_event.send(DownloadEvent::Finished);
    Ok(())
}

#[tauri::command]
pub fn cancel_update_download(
    window: WebviewWindow,
    state: State<'_, UpdateState>,
) -> Result<(), String> {
    require_main(&window)?;
    let mut session = state.0.lock().map_err(|e| e.to_string())?;
    require_owner(&session)?;
    cancel_session(&mut session, &staged_path(window.app_handle())?)
}

fn cancel_session(session: &mut UpdateSession, path: &std::path::Path) -> Result<(), String> {
    if session.installing {
        return Err("Installation is already in progress.".into());
    }
    if let Some(cancel) = session.transfer.take() {
        session.generation = session.generation.wrapping_add(1);
        let _ = cancel.send(());
    } else if session.staged {
        remove_staged(path)
            .map_err(|error| format!("Cannot cancel the scheduled update: {error}"))?;
        session.staged = false;
    } else {
        session.ready = None;
    }
    Ok(())
}

#[tauri::command]
pub fn install_downloaded_update(
    window: WebviewWindow,
    state: State<'_, UpdateState>,
    documents: State<'_, crate::SharedDocuments>,
    jobs: State<'_, crate::SharedPracticeJob>,
) -> Result<(), String> {
    require_main(&window)?;
    require_supported(window.app_handle())?;
    let documents = crate::lock_documents(&documents)?;
    let jobs = jobs.lock().map_err(|e| e.to_string())?;
    if !documents.sessions.is_empty() || jobs.is_some() {
        return Err("Close scores and finish active work before installing an update.".into());
    }
    let mut session = state.0.lock().map_err(|e| e.to_string())?;
    require_owner(&session)?;
    if session.transfer.is_some() || session.installing {
        return Err("An update operation is already in progress.".into());
    }
    let (update, bytes) = session
        .ready
        .take()
        .ok_or("Download the update before installing it.")?;
    session.installing = true;
    let result = update.install(&bytes).map_err(|e| e.to_string());
    session.installing = false;
    if result.is_err() {
        session.ready = Some((update, bytes));
    } else {
        session.owner = None;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staged_files_require_complete_bounded_metadata_and_package() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(STAGED_FILE);
        let metadata = serde_json::json!({"version": "1.2.3", "signature": "exact"});
        save_staged(&path, &metadata, b"installer").unwrap();
        assert_eq!(
            load_staged(&path).unwrap(),
            (metadata, b"installer".to_vec())
        );
        for bytes in [
            vec![],
            vec![1, 2],
            100u64.to_le_bytes().to_vec(),
            0u64.to_le_bytes().to_vec(),
        ] {
            std::fs::write(&path, bytes).unwrap();
            assert!(load_staged(&path).is_err());
        }
        assert!(save_staged(&path, &serde_json::json!({}), b"").is_err());
    }

    #[test]
    fn cancellation_invalidates_transfer_and_removes_completed_stage() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(STAGED_FILE);
        let (sender, mut receiver) = tokio::sync::oneshot::channel();
        let mut session = UpdateSession {
            transfer: Some(sender),
            generation: 1,
            ..Default::default()
        };
        cancel_session(&mut session, &path).unwrap();
        assert_eq!(session.generation, 2);
        assert!(receiver.try_recv().is_ok());
        std::fs::write(&path, b"complete").unwrap();
        session.staged = true;
        cancel_session(&mut session, &path).unwrap();
        assert!(!path.exists());
        assert!(!session.staged);
        std::fs::create_dir(&path).unwrap();
        session.staged = true;
        assert!(cancel_session(&mut session, &path).is_err());
        assert!(session.staged);
        std::fs::remove_dir(&path).unwrap();
        cancel_session(&mut session, &path).unwrap();
        assert!(!session.staged);
        cancel_session(&mut session, &path).unwrap();
    }

    // Public throwaway "test" vectors from tauri-plugin-updater's offline tests.
    const TEST_PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEE0OUVGNDc0NkZBNkU2RDcKUldUWDVxWnZkUFNlcERHV2cyYno4bjZBclFuQk13Z1FLMWdlVm9zb3JjYkx6Y3VuRCtLSitjWWU=";
    const TEST_SIGNATURE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVUWDVxWnZkUFNlcFBGMnpzanZQL0wvVFF0QkxSbldSdmFxKzRvbUsxVUV1Z3RRcC9VU3JHaUkxdjc2MnNEeTNpZDNhb3I3YlJDOUVUeWVvdDZsTUhIcldvWjVoalVNMVFjPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkxNjQ5MDUxCWZpbGU6cGF5bG9hZAl2ZXJzaW9uOjIuMC4wCkREcTJDTE9FbDVjUW1DMmVKUkhsblpQVlZpT1pzdTFDanBQS1FvZWk0QS9YQmZrRHFHTzBCK1FNcmNySGdFUTVxVjNtbmU3NXRkRldoSnBxVVhoOUFBPT0=";
    const TEST_UNVERSIONED_PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXkgRTc2MjBGMTg0MkI0RTgxRgpSV1FmNkxSQ0dBOWk1M21sWWVjTzRJelQ1MVRHUHB2V3VjTlNDaDFDQk0wUVRhTG43M1k3R0ZPMw==";
    const TEST_UNVERSIONED_SIGNATURE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIG1pbmlzaWduIHNlY3JldCBrZXkKUlVRZjZMUkNHQTlpNTU5cjNnN1YxcU55SkRBcEdpcDhNZnFjYWRJZ1Q5Q3VoVjNFTWhIb04xbUdUa1VpZEYvejdTcmxRZ1hkeThvZmpiN2JOSkp5bERPb2NyQ284S0x6WndvPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNjMzNzAwODM1CWZpbGU6dGVzdAlwcmVoYXNoZWQKd0xNRGp5OUZMQXV4WjNxNE5sRXZrZ3R5aHJyMGd0VHU2S0M0S0JKZElUYmJPZUFpMXpCSVlvMHY0aVRndDhqSnBJaWRSSm5wOTRBQlFrSkFnQW9vQlE9PQ==";

    fn signed_updater(public_key: &str) -> tauri_plugin_updater::Updater {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().plugins.0.insert(
            "updater".into(),
            serde_json::json!({ "pubkey": public_key }),
        );
        let app = tauri::test::mock_builder()
            .plugin(
                tauri_plugin_updater::Builder::new()
                    .pubkey(public_key)
                    .build(),
            )
            .build(context)
            .unwrap();
        app.updater_builder()
            .target("test-target")
            .executable_path(std::env::current_exe().unwrap())
            .endpoints(vec!["https://example.invalid/update".parse().unwrap()])
            .unwrap()
            .configure_client(|_| panic!("Restoring a saved update must not use the network"))
            .build()
            .unwrap()
    }

    fn signed_manifest() -> serde_json::Value {
        serde_json::json!({
            "version": "2.0.0",
            "platforms": {
                "test-target": {
                    "url": "https://example.invalid/package",
                    "signature": TEST_SIGNATURE
                }
            }
        })
    }

    #[test]
    fn saved_update_signatures_are_rechecked_before_installation() {
        let updater = signed_updater(TEST_PUBLIC_KEY);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(STAGED_FILE);
        let mut metadata = signed_manifest();
        metadata["config"] = serde_json::json!({"pubkey": "untrusted-key"});
        metadata["extract_path"] = serde_json::json!("untrusted-path");
        save_staged(&path, &metadata, b"test").unwrap();
        let (metadata, bytes) = load_staged(&path).unwrap();
        let update = updater.restore_update(metadata.clone()).unwrap().unwrap();
        update.verify(&bytes).unwrap();
        let verification_error = update.verify(b"tampered").unwrap_err().to_string();
        // install must reject altered bytes before reaching any platform installer.
        assert_eq!(
            update.install(b"tampered").unwrap_err().to_string(),
            verification_error
        );
        let mut forged = metadata.clone();
        forged["version"] = serde_json::json!("9.0.0");
        assert!(updater
            .restore_update(forged)
            .unwrap()
            .unwrap()
            .verify(&bytes)
            .is_err());
        let mut missing = metadata.clone();
        missing["platforms"]["test-target"]
            .as_object_mut()
            .unwrap()
            .remove("signature");
        assert!(updater.restore_update(missing).is_err());
        let mut empty = metadata.clone();
        empty["platforms"]["test-target"]["signature"] = serde_json::json!("");
        assert!(updater
            .restore_update(empty)
            .unwrap()
            .unwrap()
            .verify(&bytes)
            .is_err());
        let mut obsolete = metadata.clone();
        obsolete["version"] = serde_json::json!("0.0.0");
        assert!(updater.restore_update(obsolete).unwrap().is_none());
        let mut incompatible = metadata;
        incompatible["platforms"] = serde_json::json!({"other-platform": {"url": "https://example.invalid/package", "signature": TEST_SIGNATURE}});
        assert!(updater.restore_update(incompatible).is_err());
    }

    #[test]
    fn saved_updates_require_a_version_authenticated_by_the_signature() {
        let updater = signed_updater(TEST_UNVERSIONED_PUBLIC_KEY);
        let mut metadata = signed_manifest();
        metadata["platforms"]["test-target"]["signature"] =
            serde_json::json!(TEST_UNVERSIONED_SIGNATURE);
        assert!(updater
            .restore_update(metadata)
            .unwrap()
            .unwrap()
            .verify(b"test")
            .is_err());
    }

    #[test]
    fn updater_ownership_is_exclusive_across_file_handles() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(STAGED_FILE);
        let owner = acquire_owner(&path).unwrap();
        assert!(acquire_owner(&path).is_err());
        drop(owner);
        assert!(acquire_owner(&path).is_ok());
    }

    #[test]
    fn automatic_checks_only_offer_newer_versions_and_manual_checks_can_downgrade() {
        assert!(should_offer(Ordering::Greater, false));
        assert!(!should_offer(Ordering::Equal, false));
        assert!(!should_offer(Ordering::Less, false));
        assert!(should_offer(Ordering::Greater, true));
        assert!(!should_offer(Ordering::Equal, true));
        assert!(should_offer(Ordering::Less, true));
    }

    #[test]
    fn channels_only_select_the_expected_https_release_endpoints() {
        assert!(matches!(
            serde_json::from_str::<ReleaseChannel>("\"nightly\"").unwrap(),
            ReleaseChannel::Nightly
        ));
        assert!(serde_json::from_str::<ReleaseChannel>("\"https://example.com\"").is_err());
        assert!(ReleaseChannel::Stable
            .endpoint()
            .ends_with("/releases/latest/download/latest.json"));
        assert!(ReleaseChannel::Nightly
            .endpoint()
            .ends_with("/releases/download/nightly/latest.json"));
        for channel in [ReleaseChannel::Stable, ReleaseChannel::Nightly] {
            assert_eq!(
                tauri::Url::parse(channel.endpoint()).unwrap().host_str(),
                Some("github.com")
            );
            assert_eq!(
                tauri::Url::parse(channel.release_page()).unwrap().scheme(),
                "https"
            );
        }
    }
}
