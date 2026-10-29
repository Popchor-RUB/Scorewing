//! Opt-in Linux audio acceptance. Use a disposable profile, never real MuseScore preferences.
#![cfg(target_os = "linux")]

use practice_tracks::{export_tracks, PartSettings, PracticeOptions, PresetId, RendererConfig};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

const RATE: f64 = 24_000.0;

struct Native {
    root: tempfile::TempDir,
    ini: PathBuf,
    ffmpeg: PathBuf,
    renderer: RendererConfig,
    exports: usize,
}

impl Native {
    fn new() -> Self {
        let profile = absolute_env("SCOREWING_NATIVE_PROFILE")
            .canonicalize()
            .expect("Native profile directory must already exist");
        assert_ne!(
            profile,
            Path::new("/"),
            "A filesystem root is not a disposable native-test profile"
        );
        // Check canonical paths before creating directories or writing preferences.
        let home = absolute_env("HOME")
            .canonicalize()
            .expect("Disposable HOME must exist");
        let config = absolute_env("XDG_CONFIG_HOME")
            .canonicalize()
            .expect("Disposable XDG_CONFIG_HOME must exist");
        for path in [&home, &config] {
            assert!(
                path.starts_with(&profile),
                "HOME and XDG_CONFIG_HOME must be inside SCOREWING_NATIVE_PROFILE: {}",
                path.display()
            );
        }
        let renderer = RendererConfig {
            executable: Some(absolute_env("MUSESCORE_EXECUTABLE")),
            soundfont: Some(absolute_env("MUSESCORE_SOUNDFONT")),
        };
        let ffmpeg = absolute_env("FFMPEG_EXECUTABLE");
        let root = tempfile::Builder::new()
            .prefix("native-audio-")
            .tempdir_in(&profile)
            .unwrap();
        let ini = config.join("MuseScore/MuseScore4.ini");
        fs::create_dir_all(ini.parent().unwrap()).unwrap();
        assert!(
            ini.parent()
                .unwrap()
                .canonicalize()
                .unwrap()
                .starts_with(&profile),
            "MuseScore preference directory escaped the disposable profile"
        );
        assert!(
            !ini.symlink_metadata()
                .is_ok_and(|metadata| metadata.file_type().is_symlink()),
            "Refusing a symlink preference file"
        );
        Self {
            root,
            ini,
            ffmpeg,
            renderer,
            exports: 0,
        }
    }

    fn initial_preference(&self, value: Option<&str>) {
        fs::write(
            &self.ini,
            format!(
                "[application]\nscorewingNativeSentinel=preserve\n{}",
                value
                    .map(|v| format!("playback\\metronomeEnabled={v}\n"))
                    .unwrap_or_default()
            ),
        )
        .unwrap();
    }

    fn preference(&self) -> Option<String> {
        let text = fs::read_to_string(&self.ini).unwrap();
        assert!(
            text.contains("scorewingNativeSentinel=preserve"),
            "MuseScore lost unrelated preferences"
        );
        text.lines().find_map(|line| {
            line.strip_prefix("playback\\metronomeEnabled=")
                .map(str::to_owned)
        })
    }

    fn export(
        &mut self,
        source: &[u8],
        options: &PracticeOptions,
        original: Option<&str>,
    ) -> Vec<Vec<f32>> {
        self.initial_preference(original);
        self.exports += 1;
        let source_path = self
            .root
            .path()
            .join(format!("input-{}.mscx", self.exports));
        fs::write(&source_path, source).unwrap();
        let report = export_tracks(
            source,
            source_path.to_str().unwrap(),
            options,
            &self.renderer,
            self.root.path(),
            &self.root.path().join("recovery"),
            &AtomicBool::new(false),
            |_| {},
        )
        .expect("Native export failed to start");
        assert!(!report.cancelled);
        assert!(
            report.error.is_none(),
            "Native export failed: {:?}",
            report.error
        );
        assert_eq!(
            self.preference().as_deref(),
            original,
            "Native export did not restore metronome preference"
        );
        assert!(!self
            .root
            .path()
            .join("recovery/musescore-metronome-recovery.json")
            .exists());
        assert_eq!(
            fs::read(source_path).unwrap(),
            source,
            "Original score changed"
        );
        assert_eq!(
            report.completed.len(),
            options.parts.iter().filter(|p| p.create_track).count()
                * (usize::from(options.include_whole_song) + options.section_keys.len())
        );
        report
            .completed
            .into_iter()
            .map(|track| {
                let decoded = Command::new(&self.ffmpeg)
                    .args(["-v", "error", "-i"])
                    .arg(track.path)
                    .args(["-f", "f32le", "-ac", "1", "-ar", "24000", "-"])
                    .output()
                    .expect("Cannot start FFmpeg");
                assert!(
                    decoded.status.success(),
                    "FFmpeg failed: {}",
                    String::from_utf8_lossy(&decoded.stderr)
                );
                assert_eq!(decoded.stdout.len() % 4, 0);
                let samples: Vec<_> = decoded
                    .stdout
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                    .collect();
                assert!(samples.len() > RATE as usize && samples.iter().all(|s| s.is_finite()));
                samples
            })
            .collect()
    }
}

fn absolute_env(name: &str) -> PathBuf {
    let value = PathBuf::from(
        std::env::var_os(name)
            .unwrap_or_else(|| panic!("Set {name} to run native audio acceptance")),
    );
    assert!(value.is_absolute(), "{name} must be absolute");
    value
}

fn options() -> PracticeOptions {
    PracticeOptions {
        include_whole_song: true,
        section_keys: vec![],
        parts: (1..=3)
            .map(|index| PartSettings {
                key: format!("part-{index}"),
                foreground_sound: Some(PresetId {
                    bank: 0,
                    program: 0,
                }),
                background_sound: Some(PresetId {
                    bank: 0,
                    program: 0,
                }),
                include_in_background: index < 3,
                create_track: index < 3,
            })
            .collect(),
        background_reduction_db: 6.0,
        metronome_enabled: false,
        metronome_db: 0.0,
    }
}

// Each part occupies distinct beats so its audio can be measured independently.
fn score(mode: &str) -> Vec<u8> {
    let mut xml = String::from("<museScore version=\"4.70\"><programVersion>4.7.5</programVersion><Score><Division>480</Division>");
    for index in 1..=3 {
        xml.push_str(&format!("<Part id=\"{index}\"><Staff id=\"{index}\"><StaffType group=\"pitched\"><name>stdNormal</name></StaffType></Staff>{}<trackName>Part {index}</trackName><Instrument id=\"piano\"><longName>Part {index}</longName><shortName>P{index}</shortName><trackName>Piano</trackName><instrumentId>keyboard.piano</instrumentId><Channel><program value=\"0\"/><synti>Fluid</synti></Channel></Instrument></Part>", if index == 3 { "<show>0</show>" } else { "" }));
    }
    for index in 0..3 {
        xml.push_str(&format!("<Staff id=\"{}\">", index + 1));
        for measure in 0..2 {
            xml.push_str("<Measure><voice>");
            if measure == 0 {
                xml.push_str("<Clef><concertClefType>G</concertClefType><transposingClefType>G</transposingClefType></Clef><TimeSig><sigN>4</sigN><sigD>4</sigD></TimeSig><Tempo><tempo>2</tempo><text>120</text></Tempo>");
            }
            if mode == "flat" && index == 0 {
                if measure == 0 {
                    xml.push_str("<Spanner type=\"HairPin\"><HairPin><subtype>0</subtype><veloChange>100</veloChange></HairPin><next><location><measures>1</measures></location></next></Spanner>");
                } else {
                    xml.push_str("<Spanner type=\"HairPin\"><prev><location><measures>-1</measures></location></prev></Spanner>");
                }
            }
            for beat in 0..4 {
                if mode == "silent" && index == 0 && measure == 0 && beat == 0 {
                    xml.push_str("<Harmony><root>14</root><extension>1</extension><name>7</name><play>1</play></Harmony>");
                }
                let note = match mode {
                    "silent" => index == 2,
                    "flat" => index == 0,
                    _ => (measure * 4 + beat) % 3 == index,
                };
                if note {
                    xml.push_str(&format!("<Dynamic><subtype>{}</subtype></Dynamic><Chord><durationType>quarter</durationType>{}<Note><pitch>{}</pitch><tpc>14</tpc><velocity>{}</velocity></Note></Chord>", if beat % 2 == 0 { "p" } else { "ff" }, if mode == "flat" && beat % 2 == 1 { "<Articulation><subtype>articAccentAbove</subtype></Articulation>" } else { "" }, 72 + index * 4, if beat % 2 == 0 { 15 } else { 120 }));
                } else {
                    xml.push_str("<Rest><durationType>quarter</durationType></Rest>");
                }
            }
            xml.push_str("</voice></Measure>");
        }
        xml.push_str("</Staff>");
    }
    xml.push_str("</Score></museScore>");
    xml.into_bytes()
}

fn timing_score() -> Vec<u8> {
    let mut root = scorewing_core::parse_xml(&score("silent"), "native timing fixture").unwrap();
    for staff in root
        .get_mut_child("Score")
        .unwrap()
        .children
        .iter_mut()
        .filter_map(scorewing_core::XMLNode::as_mut_element)
        .filter(|e| e.name == "Staff")
    {
        let measures = br#"<Staff>
          <Measure len="1/4"><irregular>1</irregular><voice><TimeSig><sigN>4</sigN><sigD>4</sigD></TimeSig><Tempo><tempo>2</tempo><text>120</text></Tempo><Rest><durationType>quarter</durationType></Rest></voice></Measure>
          <Measure><startRepeat/><voice><TimeSig><sigN>3</sigN><sigD>4</sigD></TimeSig><Tempo><tempo>2</tempo><text>120</text></Tempo><Rest><durationType>measure</durationType><duration>3/4</duration></Rest></voice></Measure>
          <Measure><endRepeat>2</endRepeat><voice><Tempo><tempo>1</tempo><text>60</text></Tempo><Rest><durationType>measure</durationType><duration>3/4</duration></Rest></voice></Measure>
        </Staff>"#;
        staff.children = scorewing_core::parse_xml(measures, "native timing measures")
            .unwrap()
            .children;
    }
    let mut bytes = Vec::new();
    root.write(&mut bytes).unwrap();
    bytes
}

fn rms(samples: &[f32], start: f64, end: f64) -> f64 {
    let a = (start * RATE).round() as usize;
    let b = (end * RATE).round() as usize;
    assert!(
        b <= samples.len() && a < b,
        "Audio shorter than measurement window {start}–{end}"
    );
    (samples[a..b]
        .iter()
        .map(|s| f64::from(*s).powi(2))
        .sum::<f64>()
        / (b - a) as f64)
        .sqrt()
}

fn db_ratio(first: f64, second: f64) -> f64 {
    assert!(first > 1e-8 && second > 1e-8, "Expected audible samples");
    20.0 * (first / second).log10()
}

fn correlation(first: &[f32], second: &[f32]) -> f64 {
    let a = &first[2400..9600];
    let b = &second[2400..9600];
    let product = a
        .iter()
        .zip(b)
        .map(|(x, y)| f64::from(*x) * f64::from(*y))
        .sum::<f64>();
    let energy = |s: &[f32]| s.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>();
    product / (energy(a) * energy(b)).sqrt()
}

#[test]
#[ignore = "Requires MuseScore 4.7.x, MS Basic, FFmpeg, and an isolated Linux profile; see this test's environment checks"]
fn native_audio_acceptance() {
    let mut native = Native::new();
    let source = score("mix");
    let mut settings = options();
    let mixes = native.export(&source, &settings, Some("true"));
    let difference = db_ratio(rms(&mixes[0], 0.1, 0.3), rms(&mixes[1], 0.1, 0.3));
    assert!(
        (difference - 6.0).abs() < 0.2,
        "Foreground/background difference: {difference:.3} dB"
    );
    settings.parts.iter_mut().for_each(|p| {
        p.foreground_sound = Some(PresetId {
            bank: 0,
            program: 68,
        })
    });
    let sounds = native.export(&source, &settings, Some("false"));
    assert!(
        correlation(&sounds[0], &sounds[1]).abs() < 0.8,
        "Foreground/background sound assignment was ignored"
    );
    assert!(
        correlation(&mixes[0], &sounds[1]).abs() > 0.95,
        "Background did not retain its selected piano sound"
    );

    settings.parts.iter_mut().for_each(|p| {
        p.create_track = p.key == "part-3";
        p.include_in_background = false;
    });
    let selected_hidden = native.export(&source, &settings, None);
    assert!(
        rms(&selected_hidden[0], 1.1, 1.3) > 1e-5,
        "Explicitly selected hidden instrument is inaudible"
    );

    settings
        .parts
        .iter_mut()
        .for_each(|p| p.create_track = p.key == "part-1");
    let flat = native.export(&score("flat"), &settings, None);
    let levels: Vec<_> = (0..8)
        .map(|i| rms(&flat[0], i as f64 * 0.5 + 0.15, i as f64 * 0.5 + 0.35))
        .collect();
    let spread = db_ratio(
        levels.iter().copied().fold(0.0, f64::max),
        levels.iter().copied().fold(f64::INFINITY, f64::min),
    );
    assert!(
        spread < 1.0,
        "Dynamics/articulation volume remained audible: {spread:.3} dB"
    );

    let silent = score("silent");
    let mut clicks = None;
    for original in [Some("true"), Some("false"), None] {
        settings.metronome_enabled = false;
        let off = native.export(&silent, &settings, original);
        assert!(
            off[0].iter().all(|s| s.abs() < 1e-5),
            "Excluded hidden part or disabled native metronome leaked audio: max={}, attack={}, sustain={}",
            off[0].iter().fold(0.0_f32, |peak, sample| peak.max(sample.abs())),
            rms(&off[0], 0.01, 0.1), rms(&off[0], 0.15, 0.35)
        );
        settings.metronome_enabled = true;
        settings.metronome_db = 0.0;
        let mut on = native.export(&silent, &settings, original);
        assert!(rms(&on[0], 0.0, 3.5) > 1e-5, "Native metronome is silent");
        if clicks.is_none() {
            clicks = Some(on.remove(0));
        }
    }
    settings.metronome_db = -6.0;
    let quieter = native.export(&silent, &settings, None);
    let difference = db_ratio(
        rms(clicks.as_ref().unwrap(), 0.0, 3.5),
        rms(&quieter[0], 0.0, 3.5),
    );
    assert!(
        (difference - 6.0).abs() < 0.2,
        "Independent native metronome gain: {difference:.3} dB"
    );
    settings.metronome_db = 0.0;
    let timed = native.export(&timing_score(), &settings, None);
    // One-quarter pickup, then two repeated 3/4 bars at 120 and 60 BPM.
    let beats = [
        0.0, 0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0, 5.5, 6.0, 6.5, 7.5, 8.5,
    ];
    for beat in beats {
        let pulse = rms(&timed[0], beat + 0.01, beat + 0.1);
        let gap = rms(&timed[0], beat + 0.30, beat + 0.42);
        assert!(
            pulse > 1e-5 && pulse > gap * 2.0,
            "Native click timing failed at {beat:.1}s: pulse={pulse}, gap={gap}"
        );
    }
}

// The reference is an independently authored two-bar score, not a sliced source.
fn section_score(reference: bool) -> Vec<u8> {
    let mut root = scorewing_core::parse_xml(&score("silent"), "section fixture").unwrap();
    for staff in root
        .get_mut_child("Score")
        .unwrap()
        .children
        .iter_mut()
        .filter_map(scorewing_core::XMLNode::as_mut_element)
        .filter(|e| e.name == "Staff")
    {
        let prefix = if reference {
            ""
        } else {
            r#"
          <Measure len="1/4"><irregular>1</irregular><voice><TimeSig><sigN>3</sigN><sigD>4</sigD></TimeSig><Rest><durationType>quarter</durationType></Rest></voice></Measure>
          <Measure><startRepeat/><voice><TimeSig><sigN>3</sigN><sigD>4</sigD></TimeSig><KeySig><concertKey>2</concertKey></KeySig><Tempo><tempo>1</tempo><text>60</text></Tempo><Rest><durationType>half</durationType></Rest><Chord><durationType>quarter</durationType><Note><pitch>64</pitch><tpc>18</tpc><Spanner type="Tie"><Tie/><next><location><measures>1</measures><fractions>-1/2</fractions></location></next></Spanner></Note></Chord></voice></Measure>"#
        };
        let initial = if reference {
            "<TimeSig><sigN>3</sigN><sigD>4</sigD></TimeSig><KeySig><concertKey>2</concertKey></KeySig><Tempo><tempo>1</tempo><text>60</text></Tempo>"
        } else {
            ""
        };
        let repeat = if reference {
            ""
        } else {
            "<endRepeat>2</endRepeat>"
        };
        let tie_end = if reference {
            ""
        } else {
            r#"<Spanner type="Tie"><prev><location><measures>-1</measures><fractions>1/2</fractions></location></prev></Spanner>"#
        };
        let tie_start = if reference {
            ""
        } else {
            r#"<Spanner type="Tie"><Tie/><next><location><measures>1</measures><fractions>-1/2</fractions></location></next></Spanner>"#
        };
        let suffix = if reference {
            ""
        } else {
            r#"<Measure><voice><RehearsalMark><text>B</text></RehearsalMark><TimeSig><sigN>2</sigN><sigD>4</sigD></TimeSig><Tempo><tempo>3</tempo><text>180</text></Tempo><Chord><durationType>half</durationType><Note><pitch>64</pitch><tpc>18</tpc><Spanner type="Tie"><prev><location><measures>-1</measures><fractions>1/2</fractions></location></prev></Spanner></Note><Note><pitch>96</pitch><tpc>14</tpc></Note></Chord><Jump><jumpTo>start</jumpTo><playUntil>end</playUntil><continueAt/></Jump></voice></Measure>"#
        };
        let xml = format!(
            r#"<Staff>{prefix}
          <Measure><voice>{initial}<RehearsalMark><text>A</text></RehearsalMark><Chord><durationType>quarter</durationType><Note><pitch>64</pitch><tpc>18</tpc>{tie_end}</Note></Chord><Rest><durationType>half</durationType></Rest></voice></Measure>
          <Measure>{repeat}<voice><Rest><durationType>half</durationType></Rest><Chord><durationType>quarter</durationType><Note><pitch>64</pitch><tpc>18</tpc>{tie_start}</Note></Chord></voice></Measure>
          {suffix}</Staff>"#
        );
        staff.children = scorewing_core::parse_xml(xml.as_bytes(), "section measures")
            .unwrap()
            .children;
    }
    let mut bytes = vec![];
    root.write(&mut bytes).unwrap();
    bytes
}

#[test]
#[ignore = "Requires MuseScore 4.7.x, MS Basic, FFmpeg, and an isolated Linux profile"]
fn native_rehearsal_sections_match_standalone_score() {
    let mut native = Native::new();
    let source = section_score(false);
    let archive = practice_tracks::ScoreArchive::read_bytes(&source, "sections").unwrap();
    let sections = archive.rehearsal_sections().unwrap();
    assert_eq!(
        sections
            .iter()
            .map(|s| (s.name.as_str(), s.start_measure, s.end_measure))
            .collect::<Vec<_>>(),
        [("Intro", 1, 2), ("A", 3, 4), ("B", 5, 5)]
    );
    let mut settings = options();
    settings.parts.iter_mut().for_each(|p| {
        p.create_track = p.key == "part-1";
        p.include_in_background = false;
    });
    settings.metronome_enabled = true;
    settings.include_whole_song = false;
    settings.section_keys = vec!["measure-3".into()];
    let section = native.export(&source, &settings, None).remove(0);
    settings.include_whole_song = true;
    settings.section_keys.clear();
    let reference = native
        .export(&section_score(true), &settings, None)
        .remove(0);
    assert!(
        (section.len() as f64 - reference.len() as f64).abs() < RATE * 0.1,
        "Section duration differs from the standalone score: {} vs {}",
        section.len(),
        reference.len()
    );
    for beat in [1.0, 2.0, 3.0, 4.0] {
        let pulse = rms(&section, beat + 0.01, beat + 0.1);
        let gap = rms(&section, beat + 0.3, beat + 0.42);
        assert!(
            pulse > 1e-5 && pulse > gap * 2.0,
            "Section lost inherited 60 BPM at {beat}s"
        );
    }
    let similarity = correlation(&section, &reference);
    assert!(similarity > 0.99,
        "Section start differs from standalone score: correlation={similarity}, section rms={}, reference rms={}",
        rms(&section, 0.1, 0.3), rms(&reference, 0.1, 0.3));
    for (start, end) in [(5.1, 5.3), (6.01, 6.1), (6.2, 6.4)] {
        let difference = db_ratio(rms(&section, start, end), rms(&reference, start, end));
        assert!(
            difference.abs() < 0.2,
            "Section note/decay differs from standalone score at {start}s: {difference} dB"
        );
    }
    assert!(
        rms(&section, 6.01, 6.1) > 1e-6,
        "Final sound was cut off at the next mark"
    );
}
