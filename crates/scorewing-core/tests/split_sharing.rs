mod common;
use common::*;
use scorewing_core::{
    frac, parse_score, read_score_bytes, split_plan, split_score, split_score_with_fallbacks,
    validate_score, Event, Score, SplitFallback,
};
use xmltree::Element;

fn settings(score: &Score) -> Vec<SplitFallback> {
    split_plan(&score.parts[0], "chords")
        .unwrap()
        .into_iter()
        .map(|lane| SplitFallback {
            part_id: score.parts[0].id().to_owned(),
            voice: lane.voice,
            tone: lane.tone,
            fallback_voice: lane.fallback_voice,
        })
        .collect()
}

fn note_pitch(note: &Element) -> (String, String, String) {
    (
        text(note, "pitch/step"),
        text(note, "pitch/alter"),
        text(note, "pitch/octave"),
    )
}

fn assert_covered(source: &Event, note: &Element, output: &[Event]) {
    let mut cursor = source.start.clone();
    let matches: Vec<_> = output
        .iter()
        .filter(|event| {
            event.grace() == source.grace()
                && event
                    .notes
                    .iter()
                    .any(|copied| note_pitch(copied) == note_pitch(note))
        })
        .collect();
    if source.grace() {
        assert!(matches.iter().any(|event| event.start == cursor));
        return;
    }
    for event in matches {
        if event.start <= cursor && cursor < event.end() {
            cursor = event.end();
        }
    }
    assert!(
        cursor >= source.end(),
        "missing {:?} at {}",
        note_pitch(note),
        source.start
    );
}

#[test]
fn empire_state_of_mind_reuses_the_second_alto_lane() {
    let original = read_score_bytes(
        include_bytes!("fixtures/empire-state-of-mind-sopran-alt.mxl"),
        "empire-state-of-mind-sopran-alt.mxl",
    )
    .unwrap();
    let choices = settings(&original);
    assert_eq!(
        choices
            .iter()
            .map(|s| (s.voice.as_str(), s.tone))
            .collect::<Vec<_>>(),
        [("1", 0), ("2", 0), ("2", 1)]
    );
    for configured in [false, true] {
        let root = if configured {
            split_score_with_fallbacks(&original, "chords", None, &choices)
        } else {
            split_score(&original, "chords", None)
        }
        .unwrap()
        .root;
        validate_score(&root).unwrap();
        let result = parse_score(root).unwrap();
        assert_eq!(result.parts.len(), 4);
        assert_eq!(result.parts[0].source, original.parts[0].source);
        assert_eq!(result.parts[0].definition, original.parts[0].definition);
        for (measure, step, start, duration) in [(4, "G", 3, 1), (5, "F", 0, 4)] {
            let notes: Vec<_> = result.parts[3].measures[measure]
                .events
                .iter()
                .filter(|e| e.notes[0].get_child("rest").is_none())
                .collect();
            assert_eq!(notes.len(), 1);
            assert_eq!(text(&notes[0].notes[0], "pitch/step"), step);
            assert_eq!(text(&notes[0].notes[0], "pitch/octave"), "4");
            assert_eq!(notes[0].start, frac(start));
            assert_eq!(notes[0].duration, frac(duration));
        }
        for measure in [28, 29, 31, 32] {
            assert_ne!(
                pitches(&result.parts[2], measure),
                pitches(&result.parts[3], measure)
            );
        }
        for (index, measure) in original.parts[0].measures.iter().enumerate() {
            for event in &measure.events {
                let mut notes: Vec<_> = event
                    .notes
                    .iter()
                    .filter(|note| note.get_child("rest").is_none())
                    .collect();
                notes.sort_by_key(|note| {
                    let step = ["C", "D", "E", "F", "G", "A", "B"]
                        .iter()
                        .position(|step| *step == text(note, "pitch/step"))
                        .unwrap();
                    let alter = text(note, "pitch/alter").parse::<i32>().unwrap_or(0);
                    std::cmp::Reverse(
                        text(note, "pitch/octave").parse::<i32>().unwrap() * 12
                            + [0, 2, 4, 5, 7, 9, 11][step]
                            + alter,
                    )
                });
                for (tone, note) in notes.into_iter().enumerate() {
                    let lane = match event.voice.as_str() {
                        "1" => 1,
                        "2" => 2 + tone,
                        "3" => 3,
                        _ => unreachable!(),
                    };
                    assert_covered(event, note, &result.parts[lane].measures[index].events);
                }
            }
        }
        assert!(descendants(&result.parts[3].measures[4].source)
            .iter()
            .any(|node| node.name == "accent"));
        for index in [4, 5] {
            assert!(descendants(&result.parts[3].measures[index].source)
                .iter()
                .all(|node| !["tie", "tied", "slur"].contains(&node.name.as_str())));
        }
    }
}

fn down(step: &str, voice: &str) -> String {
    n(step).voice(voice).extra("<stem>down</stem>").xml()
}

fn chord(voice: &str) -> String {
    down("G", voice) + &n("E").voice(voice).chord().extra("<stem>down</stem>").xml()
}

fn simple(middle: String) -> Score {
    score(&[(
        "Choir",
        vec![
            n("C").octave(5).extra("<stem>up</stem>").xml() + &backup(1) + &chord("2"),
            middle,
            n("D").octave(5).extra("<stem>up</stem>").xml(),
        ],
    )])
}

#[test]
fn shared_native_voice_preserves_rests_grace_lyrics_and_directions_before_fallbacks() {
    let original = simple(
        n("E").duration(2).voice("2").extra("<stem>down</stem><lyric><text>upper</text></lyric>").xml()
            + &backup(2) + &n("C").rest().voice("3").xml()
            + &n("B").voice("3").grace().extra("<stem>down</stem>").xml()
            + "<direction><direction-type><words>lower</words></direction-type><voice>3</voice></direction>"
            + &n("A").voice("3").extra("<stem>down</stem><lyric><text>lower</text></lyric>").xml()
    );
    assert_eq!(split_plan(&original.parts[0], "voices").unwrap().len(), 3);
    let mut choices = settings(&original);
    assert_eq!(choices.len(), 3);
    choices[0].fallback_voice = Some("3".into());
    choices[2].fallback_voice = None;
    for configured in [false, true] {
        let root = if configured {
            split_score_with_fallbacks(&original, "chords", None, &choices)
        } else {
            split_score(&original, "chords", None)
        }
        .unwrap()
        .root;
        validate_score(&root).unwrap();
        let result = parse_score(root).unwrap();
        assert_eq!(result.parts.len(), 4);
        assert_eq!(pitches(&result.parts[3], 0), [["E"]]);
        assert_eq!(pitches(&result.parts[3], 1), [["rest"], ["A"]]);
        let lower = &result.parts[3].measures[1];
        assert_eq!(lower.events.iter().filter(|event| event.grace()).count(), 1);
        assert_eq!(
            text(&lower.events.last().unwrap().notes[0], "lyric/text"),
            "lower"
        );
        assert!(lower
            .extras
            .iter()
            .any(|extra| text(&extra.element, "direction-type/words") == "lower"));
        assert!(lower.events.iter().all(|event| event.notes.len() == 1));
        if configured {
            assert_eq!(pitches(&result.parts[1], 1), [["rest"], ["A"]]);
            assert_eq!(pitches(&result.parts[3], 2), [["rest"]]);
        }
    }
    choices[2].fallback_voice = Some("1".into());
    let root = split_score_with_fallbacks(&original, "chords", None, &choices)
        .unwrap()
        .root;
    validate_score(&root).unwrap();
    let result = parse_score(root).unwrap();
    assert_eq!(pitches(&result.parts[3], 1), [["rest"], ["A"]]);
    assert_eq!(pitches(&result.parts[3], 2), [["D"]]);
}

#[test]
fn incompatible_or_ambiguous_lanes_are_not_shared() {
    let cases = [
        (
            "overlapping chord",
            chord("2") + &backup(1) + &down("A", "3"),
        ),
        (
            "opposite stem",
            down("F", "2") + &backup(1) + &n("A").voice("3").extra("<stem>up</stem>").xml(),
        ),
        (
            "unknown stem",
            down("F", "2") + &backup(1) + &n("A").voice("3").xml(),
        ),
        (
            "mixed stems",
            down("F", "2")
                + &backup(1)
                + &down("A", "3")
                + &n("B").voice("3").extra("<stem>up</stem>").xml(),
        ),
        (
            "grace chord",
            n("G").voice("2").grace().extra("<stem>down</stem>").xml()
                + &n("E")
                    .voice("2")
                    .grace()
                    .chord()
                    .extra("<stem>down</stem>")
                    .xml()
                + &down("F", "2")
                + &backup(1)
                + &down("A", "3"),
        ),
        (
            "grace source",
            chord("2") + &backup(1) + &n("A").voice("3").grace().extra("<stem>down</stem>").xml(),
        ),
    ];
    for (name, middle) in cases {
        let original = simple(middle);
        assert_eq!(settings(&original).len(), 4, "{name}");
    }
    let body = chord("2") + &n("C").voice("2").chord().extra("<stem>down</stem>").xml();
    let ambiguous = score(&[(
        "S",
        vec![body, down("F", "2") + &backup(1) + &down("A", "3")],
    )]);
    assert_eq!(settings(&ambiguous).len(), 4);
}

#[test]
fn different_original_staffs_remain_rejected_at_input() {
    let original = simple(down("F", "2") + &backup(1) + &down("A", "3"));
    let mut root = original.root.clone();
    let part = root.get_mut_child("part").unwrap();
    let measure = part
        .children
        .iter_mut()
        .filter_map(xmltree::XMLNode::as_mut_element)
        .filter(|node| node.name == "measure")
        .nth(1)
        .unwrap();
    let note = measure
        .children
        .iter_mut()
        .filter_map(xmltree::XMLNode::as_mut_element)
        .find(|node| node.name == "note" && text(node, "voice") == "3")
        .unwrap();
    append(note, element("<staff>2</staff>"));
    assert!(parse_score(root)
        .unwrap_err()
        .to_string()
        .contains("multi-staff"));
}

#[test]
fn chord_ties_and_slurs_reserve_single_note_continuations_and_intermediate_measures() {
    for (start, stop) in [
        ("<tie type=\"start\"/>", "<tie type=\"stop\"/>"),
        (
            "<notations><slur type=\"start\" number=\"1\"/></notations>",
            "<notations><slur type=\"stop\" number=\"1\"/></notations>",
        ),
    ] {
        let marked = |note: Note, mark: &str| {
            note.extra(&(String::from("<stem>down</stem>") + mark))
                .xml()
        };
        for reverse in [false, true] {
            let first = if reverse {
                marked(n("E").voice("2"), start)
            } else {
                down("G", "2") + &marked(n("E").voice("2").chord(), start)
            };
            let last = if reverse {
                down("G", "2") + &marked(n("E").voice("2").chord(), stop)
            } else {
                marked(n("E").voice("2"), stop)
            };
            let original = score(&[("S", vec![first, down("A", "3"), last])]);
            assert_eq!(settings(&original).len(), 3);
            assert!(settings(&original).iter().any(|lane| lane.voice == "3"));
        }
    }
}

#[test]
fn multiple_shared_voices_must_use_different_measures() {
    let simultaneous =
        simple(down("F", "2") + &backup(1) + &down("A", "3") + &backup(1) + &down("B", "4"));
    assert_eq!(settings(&simultaneous).len(), 4);
    let separate = score(&[(
        "S",
        vec![
            n("C").extra("<stem>up</stem>").xml() + &backup(1) + &chord("2"),
            down("F", "2") + &backup(1) + &down("A", "3"),
            down("B", "4"),
        ],
    )]);
    assert_eq!(settings(&separate).len(), 3);
    let root = split_score_with_fallbacks(&separate, "chords", None, &settings(&separate))
        .unwrap()
        .root;
    validate_score(&root).unwrap();
    let result = parse_score(root).unwrap();
    assert_eq!(pitches(&result.parts[3], 1), [["A"]]);
    assert_eq!(pitches(&result.parts[3], 2), [["B"]]);
}

#[test]
fn absorbed_voice_keeps_its_ties_slurs_and_explicit_rests_on_one_lane() {
    let note = |kind: &str| {
        n("A").voice("3").extra(&format!("<stem>down</stem><notations><tied type=\"{kind}\"/><slur type=\"{kind}\" number=\"1\"/></notations>"))
            .xml().replacen("<voice>", &format!("<tie type=\"{kind}\"/><voice>"), 1)
    };
    let original = score(&[(
        "S",
        vec![
            chord("2"),
            down("F", "2") + &backup(1) + &note("start"),
            down("E", "2") + &backup(1) + &note("stop"),
            down("D", "2") + &backup(1) + &n("C").voice("3").rest().xml(),
        ],
    )]);
    let choices = settings(&original);
    assert_eq!(choices.len(), 2);
    for configured in [false, true] {
        let root = if configured {
            split_score_with_fallbacks(&original, "chords", None, &choices)
        } else {
            split_score(&original, "chords", None)
        }
        .unwrap()
        .root;
        validate_score(&root).unwrap();
        let result = parse_score(root).unwrap();
        assert_eq!(pitches(&result.parts[2], 3), [["rest"]]);
        for (index, kind) in [(1, "start"), (2, "stop")] {
            let note = &result.parts[2].measures[index].events[0].notes[0];
            assert_eq!(note_pitch(note), ("A".into(), String::new(), "4".into()));
            for tag in ["tie", "tied", "slur"] {
                assert!(descendants(note).iter().any(|node| node.name == tag
                    && node
                        .attributes
                        .get("type")
                        .is_some_and(|value| value == kind)));
            }
        }
    }
}

#[test]
fn native_relationship_ambiguities_warn_after_compaction() {
    let original = score(&[("S", vec![
        n("C").voice("2").extra("<stem>down</stem><tie type=\"start\"/><notations><slur type=\"start\" number=\"1\"/></notations>").xml()
            + &n("G").octave(3).voice("2").chord().extra("<stem>down</stem>").xml()
            + &n("C").voice("2").extra("<stem>down</stem><tie type=\"stop\"/><notations><slur type=\"stop\" number=\"1\"/></notations>").xml()
            + &n("B").octave(5).voice("2").chord().extra("<stem>down</stem>").xml(),
        down("A", "3"),
    ])]);
    assert_eq!(settings(&original).len(), 2);
    for configured in [false, true] {
        let error = if configured {
            split_score_with_fallbacks(&original, "chords", None, &settings(&original))
        } else {
            split_score(&original, "chords", None)
        }
        .unwrap()
        .warnings
        .join("\n");
        assert!(error.contains("ambiguous tie"), "{error}");
        assert!(error.contains("ambiguous slur"), "{error}");
    }
}
