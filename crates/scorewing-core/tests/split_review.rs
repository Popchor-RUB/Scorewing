mod common;
use common::*;
use scorewing_core::{
    parse_score, split_plan, split_score, split_score_with_fallbacks, validate_score, SplitFallback,
};

fn relation(kind: &str) -> String {
    format!("<notations><tied type=\"{kind}\"/><slur type=\"{kind}\" number=\"1\"/></notations>")
}

fn tied(note: Note, kind: &str) -> String {
    note.xml()
        .replacen("<voice>", &format!("<tie type=\"{kind}\"/><voice>"), 1)
}

#[test]
fn ambiguous_relations_keep_notes_and_mark_original_and_split_passages() {
    let original = score(&[(
        "Choir",
        vec![
            tied(n("C").extra(&relation("start")), "start") + &n("G").octave(3).chord().xml(),
            n("D")
                .extra("<notations><slur type=\"continue\" number=\"1\"/></notations>")
                .xml()
                + &n("A").octave(3).chord().xml(),
            tied(n("C").extra(&relation("stop")), "stop") + &n("B").octave(5).chord().xml(),
            n("F").xml().replace("<note>", "<note color=\"#0000FF\">") + &n("A").chord().xml(),
        ],
    )]);
    let source_before = original.root.clone();
    for configured in [false, true] {
        let result = if configured {
            let choices: Vec<_> = split_plan(&original.parts[0], "chords")
                .unwrap()
                .into_iter()
                .map(|lane| SplitFallback {
                    part_id: "P1".into(),
                    voice: lane.voice,
                    tone: lane.tone,
                    fallback_voice: None,
                })
                .collect();
            split_score_with_fallbacks(&original, "chords", None, &choices)
        } else {
            split_score(&original, "chords", None)
        }
        .unwrap();
        assert_eq!(result.warnings.len(), 2);
        assert!(result.warnings.iter().any(|w| w.contains("ambiguous tie")));
        assert!(result.warnings.iter().any(|w| w.contains("ambiguous slur")));
        validate_score(&result.root).unwrap();
        let output = parse_score(result.root).unwrap();
        assert_eq!(output.parts.len(), 3);
        for part in &output.parts {
            assert_eq!(part.measures.len(), 4);
            for measure in &part.measures[..3] {
                for event in &measure.events {
                    assert_eq!(event.start, scorewing_core::frac(0));
                    assert_eq!(event.duration, scorewing_core::frac(1));
                    for note in &event.notes {
                        assert!(note.get_child("pitch").is_some());
                        assert_eq!(
                            note.attributes.get("color").map(String::as_str),
                            Some("#FF0000")
                        );
                    }
                }
            }
            assert!(part.measures[3]
                .events
                .iter()
                .flat_map(|e| &e.notes)
                .all(|note| note.attributes.get("color").is_none_or(|c| c != "#FF0000")));
        }
        assert_eq!(pitches(&output.parts[1], 0), [["C"]]);
        assert_eq!(pitches(&output.parts[2], 0), [["G"]]);
        assert_eq!(pitches(&output.parts[1], 1), [["D"]]);
        assert_eq!(pitches(&output.parts[2], 1), [["A"]]);
        assert_eq!(pitches(&output.parts[1], 2), [["B"]]);
        assert_eq!(pitches(&output.parts[2], 2), [["C"]]);
        for part in &output.parts[1..] {
            assert!(descendants(&part.source)
                .iter()
                .all(|n| !["tie", "tied", "slur"].contains(&n.name.as_str())));
        }
        assert!(descendants(&output.parts[0].source)
            .iter()
            .any(|n| n.name == "slur"));
        assert_eq!(
            output.parts[0].measures[3].events[0].notes[0].attributes["color"],
            "#0000FF"
        );
        assert_eq!(
            output.parts[2].measures[3].events[0].notes[0].attributes["color"],
            "#0000FF"
        );
        assert_eq!(original.root, source_before);
    }
}

#[test]
fn shared_voice_transition_keeps_matching_ties_and_omits_uncertain_ones() {
    let original = score(&[(
        "Choir",
        vec![
            tied(n("C").extra(&relation("start")), "start") + &tied(n("E").chord(), "start"),
            tied(n("C").voice("2").extra(&relation("stop")), "stop")
                + &tied(n("G").voice("2").chord(), "stop"),
        ],
    )]);
    let result = split_score(&original, "voices", None).unwrap();
    assert!(result.warnings.iter().any(|w| w.contains("ambiguous tie")));
    validate_score(&result.root).unwrap();
    let output = parse_score(result.root).unwrap();
    for part in &output.parts[1..] {
        assert_eq!(pitches(part, 0), [["C", "E"]]);
        assert_eq!(pitches(part, 1), [["C", "G"]]);
        for measure in &part.measures {
            assert!(measure.events[0].notes[0].get_child("tie").is_some());
            assert!(measure.events[0].notes[1].get_child("tie").is_none());
            assert!(measure.events[0]
                .notes
                .iter()
                .all(|n| n.attributes.get("color").is_some_and(|c| c == "#FF0000")));
        }
    }
}

#[test]
fn review_preserves_other_voices_colors_and_complete_connections() {
    let green = |note: Note| note.xml().replace("<note>", "<note color=\"#00AA00\">");
    let original = score(&[(
        "Choir",
        vec![
            tied(n("C").extra(&relation("start")), "start")
                + &backup(1)
                + &green(n("F").voice("2")),
            tied(n("C").extra(&relation("stop")), "stop")
                + &n("G").octave(3).chord().xml()
                + &backup(1)
                + &green(n("A").voice("2").grace())
                + &green(n("F").voice("2")),
            tied(n("C").extra(&relation("start")), "start") + &n("G").octave(3).chord().xml(),
            tied(n("C").extra(&relation("stop")), "stop") + &n("G").octave(3).chord().xml(),
        ],
    )]);
    let result = split_score(&original, "chords", None).unwrap();
    assert_eq!(result.warnings.len(), 2);
    validate_score(&result.root).unwrap();
    let output = parse_score(result.root).unwrap();
    for measure in &output.parts[0].measures[..2] {
        for event in &measure.events {
            for note in &event.notes {
                let color = if event.voice == "1" {
                    "#FF0000"
                } else {
                    "#00AA00"
                };
                assert_eq!(note.attributes["color"], color);
            }
        }
    }
    // The first lane has a complete connection even though the doubled lane loses one endpoint.
    for (measure, kind) in [(0, "start"), (1, "stop"), (2, "start"), (3, "stop")] {
        let note = &output.parts[1].measures[measure].events[0].notes[0];
        assert_eq!(note.get_child("tie").unwrap().attributes["type"], kind);
        assert!(descendants(note)
            .iter()
            .any(|n| n.name == "slur" && n.attributes["type"] == kind));
        if measure >= 2 {
            assert!(!note.attributes.contains_key("color"));
        }
    }
    let unaffected_voice = output.parts.last().unwrap();
    assert!(unaffected_voice.measures[..2]
        .iter()
        .flat_map(|m| &m.events)
        .flat_map(|e| &e.notes)
        .all(|note| note.attributes.get("color").is_some_and(|c| c == "#00AA00")));
}

#[test]
fn a_sliced_transition_marks_the_whole_original_sustained_note() {
    let original = score(&[(
        "Choir",
        vec![
            n("C").duration(4).kind("whole").xml()
                + &n("E").duration(4).kind("whole").chord().xml()
                + &backup(4)
                + &n("F").voice("2").xml()
                + &n("A").voice("2").chord().xml()
                + &tied(n("F").voice("2"), "start")
                + &tied(n("A").voice("2").chord(), "start"),
        ],
    )]);
    let result = split_score(&original, "voices", None).unwrap();
    assert!(result.warnings.iter().any(|w| w.contains("ambiguous tie")));
    validate_score(&result.root).unwrap();
    let output = parse_score(result.root).unwrap();
    for part in &output.parts[..2] {
        for note in &part.measures[0].events[0].notes {
            assert_eq!(
                note.attributes.get("color").map(String::as_str),
                Some("#FF0000")
            );
        }
    }
    let events = &output.parts[2].measures[0].events;
    assert!(!events[0].notes[0].attributes.contains_key("color"));
    for event in &events[1..] {
        for note in &event.notes {
            assert_eq!(
                note.attributes.get("color").map(String::as_str),
                Some("#FF0000")
            );
        }
    }
}
