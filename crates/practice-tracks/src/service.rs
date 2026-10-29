//! Immutable score preparation and publication of complete practice tracks.
use crate::preferences::MetronomePreference;
use crate::renderer::{self, CONVERSION_TIMEOUT};
use crate::{
    CompletedTrack, ExportReport, PracticeOptions, PracticePart, Progress, RendererConfig, Result,
    ScoreArchive, SoundCatalog,
};
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::Duration;

struct RenderJob<'a> {
    score: &'a ScoreArchive,
    part: &'a PracticePart,
    path: PathBuf,
    key: String,
    name: String,
}

// Keep the desktop boundary flat; these arguments have distinct ownership and lifetimes.
#[allow(clippy::too_many_arguments)]
pub fn export_tracks(
    bytes: &[u8],
    context: &str,
    options: &PracticeOptions,
    config: &RendererConfig,
    output_parent: &Path,
    recovery_dir: &Path,
    cancelled: &AtomicBool,
    progress: impl Fn(Progress),
) -> Result<ExportReport> {
    export_with_preference(
        bytes,
        context,
        options,
        config,
        output_parent,
        cancelled,
        progress,
        || MetronomePreference::begin(recovery_dir, options.metronome_enabled),
        worker_limit(
            std::thread::available_parallelism().ok().map(usize::from),
            options
                .parts
                .iter()
                .filter(|part| part.create_track)
                .count()
                .saturating_mul(
                    options.section_keys.len() + usize::from(options.include_whole_song),
                ),
        ),
    )
}

// The preference factory keeps integration tests away from the user's settings.
#[allow(clippy::too_many_arguments)]
fn export_with_preference(
    bytes: &[u8],
    context: &str,
    options: &PracticeOptions,
    config: &RendererConfig,
    output_parent: &Path,
    cancelled: &AtomicBool,
    progress: impl Fn(Progress),
    preference: impl FnOnce() -> Result<MetronomePreference>,
    worker_count: usize,
) -> Result<ExportReport> {
    let original = ScoreArchive::read_bytes(bytes, context)?;
    let original_parts = original.parts()?;
    let sections = original.selected_sections(options)?;
    let all_sections = original.rehearsal_sections()?;
    let executable = renderer::find_executable(config.executable.as_deref())?;
    let soundfont = renderer::find_soundfont(&executable, config.soundfont.as_deref())?;
    let catalog = SoundCatalog::read(&soundfont)?;
    options.validate(&original_parts, &catalog)?;
    renderer::ensure_musescore_closed(&executable)?;
    renderer::check_version(&executable, cancelled)?;
    if cancelled.load(Ordering::Relaxed) {
        return Err("Export cancelled.".into());
    }
    if !output_parent.is_dir() {
        return Err("Select an existing output folder.".into());
    }
    let directory = create_results_folder(output_parent, context)?;
    let mut report = ExportReport {
        directory: directory.to_string_lossy().into_owned(),
        completed: vec![],
        error: None,
        cancelled: false,
    };
    let total = options.parts.iter().filter(|p| p.create_track).count()
        * (sections.len() + usize::from(options.include_whole_song));
    let mut preference = match preference() {
        Ok(guard) => guard,
        Err(error) => {
            report.error = Some(error);
            return Ok(report);
        }
    };
    let rendered = (|| {
        renderer::ensure_musescore_closed(&executable)?;
        let staging = tempfile::Builder::new()
            .prefix(".working-")
            .tempdir_in(&directory)
            .map_err(|e| format!("Cannot create export staging folder: {e}"))?;
        progress(Progress {
            stage: "Preparing score".into(),
            completed: 0,
            running: 0,
            total,
        });
        check_cancelled(cancelled)?;
        let extension = if bytes.starts_with(b"PK") {
            "mscz"
        } else {
            "mscx"
        };
        let source = staging.path().join(format!("source.{extension}"));
        fs::write(&source, bytes).map_err(|e| format!("Cannot write score snapshot: {e}"))?;
        let normalized_path = staging.path().join("normalized.mscz");
        renderer::run(
            &executable,
            &[
                "--sound-profile".into(),
                "MuseScore Basic".into(),
                "-o".into(),
                normalized_path.as_os_str().into(),
                source.as_os_str().into(),
            ],
            CONVERSION_TIMEOUT,
            cancelled,
        )?;
        let normalized_bytes = read_bounded_score(&normalized_path)?;
        let mut normalized = ScoreArchive::read_bytes(&normalized_bytes, "normalized.mscz")?;
        normalized.verify_normalized(&original)?;
        normalized.merge_assets_from(&original);
        if !sections.is_empty() && normalized.rehearsal_sections()? != all_sections {
            return Err("Normalization changed rehearsal section boundaries or labels".into());
        }
        let parts = options
            .parts
            .iter()
            .filter(|settings| settings.create_track)
            .map(|settings| {
                original_parts
                    .iter()
                    .find(|part| part.key == settings.key)
                    .ok_or_else(|| "Practice part disappeared after validation".to_owned())
            })
            .collect::<Result<Vec<_>>>()?;
        let mut ranges = vec![];
        if options.include_whole_song {
            ranges.push((None, normalized.clone()));
        }
        if !sections.is_empty() {
            let straight_path = staging.path().join("straight.mscz");
            normalized.without_repeats().write(&straight_path)?;
            for section in &sections {
                check_cancelled(cancelled)?;
                progress(Progress {
                    stage: format!("Preparing {}", section.name),
                    completed: 0,
                    running: 0,
                    total,
                });
                let region_path = staging.path().join(format!("{}.mscz", section.key));
                renderer::run(
                    &executable,
                    &[
                        "--region".into(),
                        normalized.region(section).to_string().into(),
                        "-o".into(),
                        region_path.as_os_str().into(),
                        straight_path.as_os_str().into(),
                    ],
                    CONVERSION_TIMEOUT,
                    cancelled,
                )?;
                let mut region =
                    ScoreArchive::read_bytes(&read_bounded_score(&region_path)?, "section.mscz")?;
                region.verify_region(&normalized, section)?;
                region.prepare_section(&normalized, section);
                region.merge_assets_from(&normalized);
                ranges.push((Some(section), region));
            }
        }
        let mut jobs = vec![];
        for (section, score) in &ranges {
            let (folder, key_prefix, name_prefix) = match section {
                Some(section) => (
                    format!(
                        "{:02} - {}",
                        all_sections
                            .iter()
                            .position(|s| s.key == section.key)
                            .unwrap()
                            + 1,
                        safe_filename(&section.name)
                    ),
                    format!("{}/", section.key),
                    format!("{} — ", section.name),
                ),
                None => ("00 - Whole song".into(), String::new(), String::new()),
            };
            for (index, part) in parts.iter().enumerate() {
                jobs.push(RenderJob {
                    score,
                    part,
                    path: directory.join(&folder).join(format!(
                        "{:02} - {}.mp3",
                        index + 1,
                        safe_filename(&part.name)
                    )),
                    key: format!("{key_prefix}{}", part.key),
                    name: format!("{name_prefix}{}", part.name),
                });
            }
        }
        let stopped = AtomicBool::new(false);
        let workers = worker_count.max(1).min(total);
        let (job_sender, job_receiver) = mpsc::sync_channel::<usize>(workers);
        let job_receiver = Mutex::new(job_receiver);
        let (result_sender, result_receiver) = mpsc::channel();
        let result = std::thread::scope(|scope| {
            let mut handles = vec![];
            let mut error = None;
            for worker in 0..workers {
                let sender = result_sender.clone();
                let receiver = &job_receiver;
                let stopped = &stopped;
                let catalog = &catalog;
                let executable = &executable;
                let staging = staging.path();
                let jobs = &jobs;
                match std::thread::Builder::new()
                    .name(format!("practice-render-{worker}"))
                    .spawn_scoped(scope, move || {
                        loop {
                            let received = receiver.lock().unwrap().recv();
                            let Ok(index) = received else { break };
                            if stopped.load(Ordering::Relaxed) {
                                break;
                            }
                            let result =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    let track_dir = staging.join(format!("track-{:04}", index + 1));
                                    fs::create_dir(&track_dir).map_err(|e| {
                                        format!("Cannot create track staging folder: {e}")
                                    })?;
                                    let prepared_path = track_dir.join("practice.mscz");
                                    let mp3_path = track_dir.join("rendered.mp3");
                                    jobs[index]
                                        .score
                                        .mix(options, &jobs[index].part.key, catalog)?
                                        .write(&prepared_path)?;
                                    check_cancelled(stopped)?;
                                    renderer::run(
                                        executable,
                                        &[
                                            OsString::from("-b"),
                                            OsString::from("192"),
                                            OsString::from("-o"),
                                            mp3_path.as_os_str().into(),
                                            prepared_path.as_os_str().into(),
                                        ],
                                        CONVERSION_TIMEOUT,
                                        stopped,
                                    )?;
                                    renderer::validate_mp3(&mp3_path)?;
                                    Ok::<_, String>(mp3_path)
                                }))
                                .unwrap_or_else(|_| {
                                    Err("Practice-track render worker panicked.".into())
                                });
                            let failed = result.is_err();
                            // Queue the cause before stopping siblings, whose cancellation is secondary.
                            if sender.send((index, result)).is_err() {
                                break;
                            }
                            if failed {
                                stopped.store(true, Ordering::Relaxed);
                                break;
                            }
                        }
                    }) {
                    Ok(handle) => handles.push(handle),
                    Err(cause) => {
                        error = Some(format!(
                            "Cannot start practice-track render worker: {cause}"
                        ));
                        stopped.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
            drop(result_sender);
            let mut job_sender = Some(job_sender);
            let mut next = 0;
            let mut running = 0;
            if error.is_none() {
                for (index, job) in jobs.iter().enumerate().take(workers) {
                    job_sender.as_ref().unwrap().send(index).unwrap();
                    next += 1;
                    running += 1;
                    progress(Progress {
                        stage: format!("Rendering {}", job.name),
                        completed: 0,
                        running,
                        total,
                    });
                }
            } else {
                job_sender.take();
            }
            loop {
                if cancelled.load(Ordering::Relaxed) && error.is_none() {
                    error = Some("Export cancelled.".into());
                    stopped.store(true, Ordering::Relaxed);
                }
                if error.is_some() || next == total {
                    job_sender.take();
                }
                match result_receiver.recv_timeout(Duration::from_millis(50)) {
                    Ok((index, result)) => {
                        running -= 1;
                        if error.is_some() {
                            progress(Progress {
                                stage: "Stopping exports".into(),
                                completed: report.completed.len(),
                                running,
                                total,
                            });
                            continue;
                        }
                        let published = result.and_then(|mp3_path| {
                            check_cancelled(cancelled)?;
                            let job = &jobs[index];
                            fs::create_dir_all(job.path.parent().unwrap())
                                .map_err(|e| format!("Cannot create section output folder: {e}"))?;
                            publish(&mp3_path, &job.path)?;
                            report.completed.push(CompletedTrack {
                                key: job.key.clone(),
                                name: job.name.clone(),
                                path: job.path.to_string_lossy().into_owned(),
                            });
                            progress(Progress {
                                stage: format!("Created {}", job.name),
                                completed: report.completed.len(),
                                running,
                                total,
                            });
                            Ok(())
                        });
                        if let Err(cause) = published {
                            error = Some(cause);
                            stopped.store(true, Ordering::Relaxed);
                            job_sender.take();
                            progress(Progress {
                                stage: "Stopping exports".into(),
                                completed: report.completed.len(),
                                running,
                                total,
                            });
                        } else if next < total
                            && !stopped.load(Ordering::Relaxed)
                            && !cancelled.load(Ordering::Relaxed)
                        {
                            if job_sender.as_ref().unwrap().send(next).is_err() {
                                error = Some(
                                    "Practice-track render workers stopped unexpectedly.".into(),
                                );
                                stopped.store(true, Ordering::Relaxed);
                            } else {
                                running += 1;
                                progress(Progress {
                                    stage: format!("Rendering {}", jobs[next].name),
                                    completed: report.completed.len(),
                                    running,
                                    total,
                                });
                                next += 1;
                            }
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
            for handle in handles {
                if handle.join().is_err() && error.is_none() {
                    error = Some("Practice-track render worker panicked.".into());
                }
            }
            if error.is_none() && report.completed.len() != total {
                error = Some(
                    "Practice-track render workers stopped before completing the export.".into(),
                );
            }
            error.map_or(Ok(()), Err)
        });
        report
            .completed
            .sort_by_key(|track| jobs.iter().position(|part| part.key == track.key));
        result
    })();
    if let Err(error) = rendered {
        report.cancelled = cancelled.load(Ordering::Relaxed);
        report.error = Some(error);
    }
    if let Err(restoration) = preference.restore() {
        report.error = Some(match report.error.take() {
            Some(error) => format!("{error}\n{restoration}"),
            None => restoration,
        });
    }
    progress(Progress {
        stage: if report.cancelled {
            "Rendering cancelled"
        } else if report.error.is_some() {
            "Rendering stopped"
        } else {
            "Practice tracks complete"
        }
        .into(),
        completed: report.completed.len(),
        running: 0,
        total,
    });
    Ok(report)
}

fn worker_limit(cpu_count: Option<usize>, selected: usize) -> usize {
    selected.min((cpu_count.unwrap_or(1) / 2).max(1))
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<()> {
    if cancelled.load(Ordering::Relaxed) {
        Err("Export cancelled.".into())
    } else {
        Ok(())
    }
}

fn read_bounded_score(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let file = File::open(path)
        .map_err(|e| format!("MuseScore did not create the normalized score: {e}"))?;
    let mut bytes = vec![];
    file.take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err("Normalized score exceeds the 64 MiB limit.".into());
    }
    Ok(bytes)
}

fn create_results_folder(parent: &Path, context: &str) -> Result<PathBuf> {
    let stem = Path::new(context)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Score");
    let base = format!("{} - Practice Tracks", safe_filename(stem));
    for suffix in 0..10_000 {
        let path = parent.join(if suffix == 0 {
            base.clone()
        } else {
            format!("{base} ({})", suffix + 1)
        });
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("Cannot create practice track folder: {error}")),
        }
    }
    Err("Too many existing practice track folders; select another output folder.".into())
}

fn safe_filename(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .scan(0, |bytes, c| {
            *bytes += c.len_utf8();
            (*bytes <= 160).then_some(c)
        })
        .collect();
    let mut clean = clean.trim_matches([' ', '.']).to_owned();
    if clean.is_empty() {
        clean = "Instrument".into();
    }
    let first = clean.split('.').next().unwrap_or("").to_ascii_uppercase();
    if ["CON", "PRN", "AUX", "NUL"].contains(&first.as_str())
        || (first.len() == 4
            && (first.starts_with("COM") || first.starts_with("LPT"))
            && matches!(first.as_bytes()[3], b'1'..=b'9'))
    {
        clean.insert(0, '_');
    }
    clean
}

fn publish(source: &Path, destination: &Path) -> Result<()> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(source)
        .and_then(|f| f.sync_all())
        .map_err(|e| format!("Cannot sync completed MP3: {e}"))?;
    // tempfile uses a native no-replace rename where available, including removable drives.
    let temporary = tempfile::TempPath::try_from_path(source)
        .map_err(|e| format!("Cannot prepare completed MP3 publication: {e}"))?;
    match temporary.persist_noclobber(destination) {
        Ok(()) => Ok(()),
        Err(error) => {
            let message = format!(
                "Cannot publish {} without overwriting a file: {}",
                destination.display(),
                error.error
            );
            let _ = error.path.keep();
            Err(message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    fn export_fixture(
        fail_second: bool,
        cancel_after_first: bool,
    ) -> (ExportReport, Vec<u8>, Vec<u8>, tempfile::TempDir) {
        use crate::{PartSettings, PresetId};
        use std::io::{Cursor, Write};
        use std::os::unix::fs::PermissionsExt;
        use zip::{write::SimpleFileOptions, ZipWriter};
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("fake-renderer");
        let script = format!(
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then echo 'MuseScore Studio 4.7.5'; exit 0; fi
if [ "$1" = "--sound-profile" ]; then cp "$5" "$4"; exit $?; fi
track="$(basename "$(dirname "$5")")"
touch "$(dirname "$0")/$track"
if [ "$track" = "track-0002" ] && [ "{}" = "true" ]; then echo 'deliberate render failure' >&2; exit 9; fi
printf '\377\373\260\000' > "$4"
dd if=/dev/zero bs=200 count=1 >> "$4" 2>/dev/null
"#,
            fail_second
        );
        fs::write(&executable, script).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let mut rows = vec![0; 76];
        rows[..5].copy_from_slice(b"Piano");
        rows[38..41].copy_from_slice(b"EOP");
        let mut pdta = b"pdta".to_vec();
        pdta.extend(b"phdr");
        pdta.extend((rows.len() as u32).to_le_bytes());
        pdta.extend(rows);
        let mut riff = b"sfbkLIST".to_vec();
        riff.extend((pdta.len() as u32).to_le_bytes());
        riff.extend(pdta);
        let mut font = b"RIFF".to_vec();
        font.extend((riff.len() as u32).to_le_bytes());
        font.extend(riff);
        let soundfont = dir.path().join("MS Basic.sf3");
        fs::write(&soundfont, font).unwrap();
        let xml = br#"<museScore version="4.70"><Score><Division>480</Division><Part id="1"><Staff id="1"/><trackName>Same</trackName><Instrument id="piano"><Channel><program value="0"/></Channel></Instrument></Part><Part id="2"><Staff id="2"/><trackName>Same</trackName><Instrument id="piano"><Channel><program value="0"/></Channel></Instrument></Part><Staff id="1"/><Staff id="2"/></Score></museScore>"#;
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in [
            ("META-INF/container.xml", br#"<container><rootfiles><rootfile full-path="score.mscx"/></rootfiles></container>"#.as_slice()),
            ("score.mscx", xml.as_slice()),
            ("audiosettings.json", br#"{"tracks":[{"partId":"999","instrumentId":"metronome","in":{"resourceMeta":{"id":"native"}}}]}"#.as_slice()),
            ("assets/preserve.bin", b"unchanged".as_slice()),
        ] { archive.start_file(name, SimpleFileOptions::default()).unwrap(); archive.write_all(data).unwrap(); }
        let bytes = archive.finish().unwrap().into_inner();
        let snapshot = bytes.clone();
        let options = PracticeOptions {
            include_whole_song: true,
            section_keys: vec![],
            parts: (1..=2)
                .map(|n| PartSettings {
                    key: format!("part-{n}"),
                    foreground_sound: Some(PresetId {
                        bank: 0,
                        program: 0,
                    }),
                    background_sound: Some(PresetId {
                        bank: 0,
                        program: 0,
                    }),
                    include_in_background: true,
                    create_track: true,
                })
                .collect(),
            background_reduction_db: 6.,
            metronome_enabled: true,
            metronome_db: -6.,
        };
        let preferences = dir.path().join("fake-preferences.ini");
        fs::write(
            &preferences,
            "[application]\nplayback\\metronomeEnabled=false\nunrelated=42\n",
        )
        .unwrap();
        let before_preferences = fs::read(&preferences).unwrap();
        let cancelled = AtomicBool::new(false);
        let report = export_with_preference(
            &bytes,
            "score.mscz",
            &options,
            &RendererConfig {
                executable: Some(executable),
                soundfont: Some(soundfont),
            },
            dir.path(),
            &cancelled,
            |progress| {
                if cancel_after_first && progress.completed == 1 {
                    cancelled.store(true, Ordering::Relaxed);
                }
            },
            || MetronomePreference::begin_test(&dir.path().join("recovery"), &preferences, true),
            1,
        )
        .unwrap();
        assert_eq!(bytes, snapshot);
        assert_eq!(fs::read(&preferences).unwrap(), before_preferences);
        assert!(!dir
            .path()
            .join("recovery/musescore-metronome-recovery.json")
            .exists());
        (report, bytes, vec![], dir)
    }

    #[test]
    #[cfg(unix)]
    fn fake_renderer_exports_duplicate_names_and_restores_preferences() {
        let (report, _, _, _dir) = export_fixture(false, false);
        assert!(report.error.is_none(), "{:?}", report.error);
        assert_eq!(report.completed.len(), 2);
        assert_ne!(report.completed[0].path, report.completed[1].path);
        assert!(report
            .completed
            .iter()
            .all(|track| Path::new(&track.path).is_file()));
        assert_eq!(
            fs::read_dir(Path::new(&report.directory).join("00 - Whole song"))
                .unwrap()
                .count(),
            2
        );
    }

    #[test]
    #[cfg(unix)]
    fn fake_renderer_preserves_completed_files_on_failure_or_cancellation() {
        for (fail_second, cancel) in [(true, false), (false, true)] {
            let (report, _, _, _dir) = export_fixture(fail_second, cancel);
            assert_eq!(report.completed.len(), 1);
            assert!(Path::new(&report.completed[0].path).is_file());
            assert!(report.error.is_some());
            assert_eq!(report.cancelled, cancel);
            assert_eq!(fs::read_dir(&report.directory).unwrap().count(), 1);
        }
    }

    #[test]
    fn filenames_are_portable_and_duplicate_results_are_preserved() {
        assert_eq!(safe_filename(" .. "), "Instrument");
        assert_eq!(safe_filename("CON"), "_CON");
        assert_eq!(safe_filename("NUL.wav"), "_NUL.wav");
        assert_eq!(safe_filename("../Soprano: 1?"), "_Soprano_ 1_");
        let dir = tempfile::tempdir().unwrap();
        let one = create_results_folder(dir.path(), "Score.mscz").unwrap();
        let two = create_results_folder(dir.path(), "Score.mscz").unwrap();
        assert_ne!(one, two);
    }
    #[test]
    fn publication_never_overwrites_and_keeps_original_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let destination = dir.path().join("destination");
        fs::write(&source, b"audio").unwrap();
        fs::write(&destination, b"existing").unwrap();
        assert!(publish(&source, &destination).is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"existing");
        assert_eq!(fs::read(&source).unwrap(), b"audio");
        fs::remove_file(&destination).unwrap();
        publish(&source, &destination).unwrap();
        assert_eq!(fs::read(destination).unwrap(), b"audio");
        assert!(!source.exists());
    }
}

#[cfg(all(test, unix))]
mod parallel_tests;
