use super::*;
use crate::{PartSettings, PresetId};
use std::io::{Cursor, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::thread;
use std::time::{Duration, Instant};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

struct Fixture {
    dir: tempfile::TempDir,
    bytes: Vec<u8>,
    options: PracticeOptions,
    config: RendererConfig,
    preferences: PathBuf,
    original_preferences: Vec<u8>,
}

impl Fixture {
    fn new(parts: usize, fail: usize) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("fake-renderer");
        let script = format!(
            r#"#!/bin/sh
root="$(dirname "$0")"
if [ "$1" = "--version" ]; then echo 'MuseScore Studio 4.7.5'; exit 0; fi
if [ "$1" = "--sound-profile" ]; then echo normalized >> "$root/normalizations"; cp "$5" "$4"; exit $?; fi
track="$(basename "$(dirname "$5")")"
n="${{track#track-}}"
cp "$5" "$root/mix-$n.mscz" || exit 92
printf '%s\n%s\n' "$$" "$5" > "$root/started-$n"
i=0
while [ ! -f "$root/release-$n" ]; do
    i=$((i+1))
    if [ "$i" -gt 1000 ]; then echo 'test barrier timed out' >&2; exit 93; fi
    sleep 0.01
done
if [ "$n" = "{fail:04}" ]; then echo 'deliberate parallel render failure' >&2; exit 9; fi
printf '\377\373\260\000' > "$4"
dd if=/dev/zero bs=200 count=1 >> "$4" 2>/dev/null
printf '%s' "$n" >> "$4"
"#
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

        let mut xml = String::from("<museScore version=\"4.70\"><Score><Division>480</Division>");
        for n in 1..=parts {
            xml.push_str(&format!("<Part id=\"{n}\"><Staff id=\"{n}\"/><trackName>Same</trackName><Instrument id=\"piano\"><Channel><program value=\"0\"/></Channel></Instrument></Part>"));
        }
        for n in 1..=parts {
            xml.push_str(&format!("<Staff id=\"{n}\"/>"));
        }
        xml.push_str("</Score></museScore>");
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in [
            ("META-INF/container.xml", br#"<container><rootfiles><rootfile full-path="score.mscx"/></rootfiles></container>"#.as_slice()),
            ("score.mscx", xml.as_bytes()),
            ("audiosettings.json", br#"{"tracks":[{"partId":"999","instrumentId":"metronome","in":{"resourceMeta":{"id":"native"}}}]}"#.as_slice()),
            ("assets/preserve.bin", b"unchanged".as_slice()),
        ] {
            archive.start_file(name, SimpleFileOptions::default()).unwrap();
            archive.write_all(data).unwrap();
        }
        let bytes = archive.finish().unwrap().into_inner();
        let options = PracticeOptions {
            parts: (1..=parts)
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
        let original_preferences =
            b"[application]\nplayback\\metronomeEnabled=false\nunrelated=42\n".to_vec();
        fs::write(&preferences, &original_preferences).unwrap();
        Self {
            dir,
            bytes,
            options,
            config: RendererConfig {
                executable: Some(executable),
                soundfont: Some(soundfont),
            },
            preferences,
            original_preferences,
        }
    }

    fn run(
        &self,
        workers: usize,
        cancelled: &AtomicBool,
        progress: impl Fn(Progress),
    ) -> ExportReport {
        let finished = AtomicBool::new(false);
        thread::scope(|scope| {
            scope.spawn(|| {
                let deadline = Instant::now() + Duration::from_secs(20);
                let mut changed = false;
                while !finished.load(Ordering::Acquire) {
                    assert!(Instant::now() < deadline, "export did not finish");
                    let restored = fs::read(&self.preferences).ok().as_deref()
                        == Some(self.original_preferences.as_slice());
                    changed |= !restored;
                    if changed && restored {
                        self.assert_renderers_stopped();
                        return;
                    }
                    thread::sleep(Duration::from_millis(1));
                }
            });
            let report = export_with_preference(
                &self.bytes,
                "score.mscz",
                &self.options,
                &self.config,
                self.dir.path(),
                cancelled,
                progress,
                || {
                    MetronomePreference::begin_test(
                        &self.dir.path().join("recovery"),
                        &self.preferences,
                        true,
                    )
                },
                workers,
            );
            finished.store(true, Ordering::Release);
            report.unwrap()
        })
    }

    fn assert_clean(&self) {
        assert_eq!(
            fs::read(&self.preferences).unwrap(),
            self.original_preferences
        );
        assert!(!self
            .dir
            .path()
            .join("recovery/musescore-metronome-recovery.json")
            .exists());
        self.assert_renderers_stopped();
        for entry in fs::read_dir(self.dir.path()).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name().to_string_lossy().starts_with("started-") {
                let data = fs::read_to_string(entry.path()).unwrap();
                assert!(
                    !Path::new(data.lines().nth(1).unwrap()).exists(),
                    "staged input survived cleanup"
                );
            }
        }
    }

    fn assert_renderers_stopped(&self) {
        for entry in fs::read_dir(self.dir.path()).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name().to_string_lossy().starts_with("started-") {
                let data = fs::read_to_string(entry.path()).unwrap();
                let pid: i32 = data.lines().next().unwrap().parse().unwrap();
                assert_eq!(
                    unsafe { libc::kill(pid, 0) },
                    -1,
                    "renderer {pid} still running when preferences restored"
                );
            }
        }
    }
}

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(5));
    }
}

fn started(root: &Path, n: usize) {
    wait_for(&root.join(format!("started-{n:04}")));
}

fn release(root: &Path, n: usize) {
    fs::write(root.join(format!("release-{n:04}")), []).unwrap();
}

fn archive_entry(path: &Path, entry: &str) -> Vec<u8> {
    let mut archive = ZipArchive::new(File::open(path).unwrap()).unwrap();
    let mut bytes = Vec::new();
    archive
        .by_name(entry)
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    bytes
}

#[test]
fn worker_count_uses_half_available_cpus_and_caps_at_selection() {
    for (cpus, expected) in [
        (None, 1),
        (Some(1), 1),
        (Some(2), 1),
        (Some(3), 1),
        (Some(4), 2),
        (Some(8), 4),
    ] {
        assert_eq!(worker_limit(cpus, 10), expected);
        assert_eq!(worker_limit(cpus, 1), 1);
        assert_eq!(worker_limit(cpus, 0), 0);
    }
    assert_eq!(worker_limit(Some(8), 3), 3);
}

#[test]
fn two_workers_overlap_bound_concurrency_and_publish_in_completion_order() {
    let fixture = Fixture::new(3, 0);
    let cancelled = AtomicBool::new(false);
    let progress = std::cell::RefCell::new(Vec::new());
    let report = thread::scope(|scope| {
        scope.spawn(|| {
            let root = fixture.dir.path();
            started(root, 1);
            started(root, 2);
            assert!(!root.join("started-0003").exists());
            release(root, 2);
            started(root, 3);
            release(root, 3);
            // Publication acknowledges completion before the first render may finish.
            wait_for(&root.join("score - Practice Tracks/03 - Same.mp3"));
            release(root, 1);
        });
        fixture.run(2, &cancelled, |p| progress.borrow_mut().push(p))
    });
    assert!(report.error.is_none(), "{:?}", report.error);
    assert_eq!(
        report
            .completed
            .iter()
            .map(|t| t.key.as_str())
            .collect::<Vec<_>>(),
        ["part-1", "part-2", "part-3"]
    );
    for (index, track) in report.completed.iter().enumerate() {
        let bytes = fs::read(&track.path).unwrap();
        assert!(bytes.ends_with(format!("{:04}", index + 1).as_bytes()));
    }
    let events = progress.borrow();
    assert!(events
        .windows(2)
        .all(|pair| pair[0].completed <= pair[1].completed));
    assert_eq!(events.last().unwrap().stage, "Practice tracks complete");
    assert_eq!(events.last().unwrap().completed, 3);
    assert_eq!(
        fs::read_to_string(fixture.dir.path().join("normalizations")).unwrap(),
        "normalized\n"
    );
    let mix1 = fixture.dir.path().join("mix-0001.mscz");
    let mix2 = fixture.dir.path().join("mix-0002.mscz");
    assert_ne!(
        archive_entry(&mix1, "audiosettings.json"),
        archive_entry(&mix2, "audiosettings.json")
    );
    assert_eq!(archive_entry(&mix1, "assets/preserve.bin"), b"unchanged");
    assert_eq!(fs::read_dir(&report.directory).unwrap().count(), 3);
    fixture.assert_clean();
}

#[test]
fn one_worker_waits_for_each_track() {
    let fixture = Fixture::new(2, 0);
    let cancelled = AtomicBool::new(false);
    let report = thread::scope(|scope| {
        scope.spawn(|| {
            let root = fixture.dir.path();
            started(root, 1);
            assert!(!root.join("started-0002").exists());
            release(root, 1);
            started(root, 2);
            wait_for(&root.join("score - Practice Tracks/01 - Same.mp3"));
            release(root, 2);
        });
        fixture.run(1, &cancelled, |_| {})
    });
    assert!(report.error.is_none(), "{:?}", report.error);
    assert_eq!(report.completed.len(), 2);
    fixture.assert_clean();
}

#[test]
fn render_failure_stops_blocked_sibling_and_keeps_original_error() {
    let fixture = Fixture::new(3, 1);
    let cancelled = AtomicBool::new(false);
    let report = thread::scope(|scope| {
        scope.spawn(|| {
            started(fixture.dir.path(), 1);
            started(fixture.dir.path(), 2);
            release(fixture.dir.path(), 1);
        });
        fixture.run(2, &cancelled, |_| {})
    });
    assert!(
        report
            .error
            .as_deref()
            .unwrap()
            .contains("deliberate parallel render failure"),
        "{:?}",
        report.error
    );
    assert!(!report.cancelled);
    assert!(report.completed.is_empty());
    assert!(!fixture.dir.path().join("started-0003").exists());
    fixture.assert_clean();
}

#[test]
fn render_failure_preserves_a_track_published_before_failure() {
    let fixture = Fixture::new(3, 2);
    let cancelled = AtomicBool::new(false);
    let report = thread::scope(|scope| {
        scope.spawn(|| {
            let root = fixture.dir.path();
            started(root, 1);
            started(root, 2);
            release(root, 1);
            started(root, 3);
            wait_for(&root.join("score - Practice Tracks/01 - Same.mp3"));
            release(root, 2);
        });
        fixture.run(2, &cancelled, |_| {})
    });
    assert!(report
        .error
        .as_deref()
        .unwrap()
        .contains("deliberate parallel render failure"));
    assert!(!report.cancelled);
    assert_eq!(report.completed.len(), 1);
    assert!(Path::new(&report.completed[0].path).is_file());
    assert_eq!(fs::read_dir(&report.directory).unwrap().count(), 1);
    fixture.assert_clean();
}

#[test]
fn cancellation_keeps_published_track_and_stops_active_sibling() {
    let fixture = Fixture::new(3, 0);
    let cancelled = AtomicBool::new(false);
    let report = thread::scope(|scope| {
        scope.spawn(|| {
            started(fixture.dir.path(), 1);
            started(fixture.dir.path(), 2);
            release(fixture.dir.path(), 1);
        });
        fixture.run(2, &cancelled, |p| {
            if p.completed == 1 {
                cancelled.store(true, Ordering::Relaxed);
            }
        })
    });
    assert!(report.cancelled);
    assert_eq!(report.completed.len(), 1);
    assert!(Path::new(&report.completed[0].path).is_file());
    assert_eq!(fs::read_dir(&report.directory).unwrap().count(), 1);
    fixture.assert_clean();
}

#[test]
fn user_cancellation_interrupts_blocked_renderers_without_a_completed_result() {
    let fixture = Fixture::new(3, 0);
    let cancelled = AtomicBool::new(false);
    let report = thread::scope(|scope| {
        scope.spawn(|| {
            started(fixture.dir.path(), 1);
            started(fixture.dir.path(), 2);
            cancelled.store(true, Ordering::Relaxed);
        });
        fixture.run(2, &cancelled, |_| {})
    });
    assert!(report.cancelled);
    assert!(report.completed.is_empty());
    assert_eq!(fs::read_dir(&report.directory).unwrap().count(), 0);
    assert!(!fixture.dir.path().join("started-0003").exists());
    fixture.assert_clean();
}

#[test]
fn publication_conflict_preserves_existing_file_and_stops_sibling() {
    let fixture = Fixture::new(2, 0);
    let cancelled = AtomicBool::new(false);
    let conflict = fixture
        .dir
        .path()
        .join("score - Practice Tracks/01 - Same.mp3");
    let report = thread::scope(|scope| {
        scope.spawn(|| {
            started(fixture.dir.path(), 1);
            started(fixture.dir.path(), 2);
            fs::write(&conflict, b"existing user file").unwrap();
            release(fixture.dir.path(), 1);
        });
        fixture.run(2, &cancelled, |_| {})
    });
    assert!(report.error.is_some());
    assert!(!report.cancelled);
    assert!(report.completed.is_empty());
    assert_eq!(fs::read(conflict).unwrap(), b"existing user file");
    fixture.assert_clean();
}
