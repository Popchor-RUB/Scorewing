mod common;
use common::*;
use scorewing_core::{
    merge::{merge_score, merge_score_with_stems},
    model::{frac, parse_score, Score},
};
use std::collections::HashSet;

fn merged(parts: &[(&str, Vec<String>)]) -> Score {
    let output = merge_score(&score(parts), &groups(&[&["P1", "P2"]])).unwrap();
    scorewing_core::validate_score(&output).unwrap();
    parse_score(output).unwrap()
}

#[test]
fn independent_source_voices_are_preserved_across_groups_and_measures() {
    let polyphony = || n("C").duration(3).voice("7").xml() + &backup(2) + &n("F").voice("9").xml();
    let input = score(&[
        ("Soprano", vec![polyphony(), polyphony()]),
        ("Alto", vec![polyphony(), polyphony()]),
        ("Tenor", vec![polyphony(), polyphony()]),
        (
            "Bass",
            vec![n("G").duration(3).xml(), n("G").duration(3).xml()],
        ),
    ]);
    let output = merge_score(&input, &groups(&[&["P1", "P2"], &["P3", "P4"]])).unwrap();
    scorewing_core::validate_score(&output).unwrap();
    let result = parse_score(output).unwrap();
    for measure in &result.parts[0].measures {
        assert_eq!(measure.length, frac(3));
        assert_eq!(
            measure
                .events
                .iter()
                .map(|e| e.voice.as_str())
                .collect::<HashSet<_>>(),
            HashSet::from(["1", "2", "3", "4"])
        );
        for event in &measure.events {
            assert_eq!(
                (event.start.clone(), event.duration.clone()),
                if text(&event.notes[0], "pitch/step") == "C" {
                    (frac(0), frac(3))
                } else {
                    (frac(1), frac(1))
                }
            );
        }
    }
    for measure in &result.parts[1].measures {
        assert_eq!(
            measure
                .events
                .iter()
                .map(|e| e.voice.as_str())
                .collect::<HashSet<_>>(),
            HashSet::from(["1", "2", "3"])
        );
    }
}

#[test]
fn ordered_groups_anchor_at_first_score_position_and_keep_unselected_parts() {
    let input = score(&[
        ("Soprano", vec![n("C").xml()]),
        ("Alto", vec![n("E").xml()]),
        ("Tenor", vec![n("G").xml()]),
        ("Bass", vec![n("C").octave(3).xml()]),
    ]);
    let output = merge_score(&input, &groups(&[&["P2", "P1"], &["P3", "P4"]])).unwrap();
    scorewing_core::validate_score(&output).unwrap();
    let result = parse_score(output).unwrap();
    assert_eq!(names(&result), ["Alto / Soprano", "Tenor / Bass"]);
    assert_eq!(
        event_notes(&result.parts[0].measures[0].events),
        [
            ("1", "E".into(), "up".into()),
            ("2", "C".into(), "down".into())
        ]
    );
    let input = score(&[
        ("A", vec![n("C").xml()]),
        ("Keep", vec![n("D").xml()]),
        ("B", vec![n("E").xml()]),
    ]);
    let output = merge_score(&input, &groups(&[&["P3", "P1"]])).unwrap();
    scorewing_core::validate_score(&output).unwrap();
    let result = parse_score(output).unwrap();
    assert_eq!(names(&result), ["B / A", "Keep"]);
    assert_eq!(result.parts[1].source, input.parts[1].source);
}

#[test]
fn invalid_groups_rejected_by_backend() {
    let input = score(&[
        ("Same", vec![n("C").xml()]),
        ("Same", vec![n("C").xml()]),
        ("Other", vec![n("C").xml()]),
    ]);
    for (groups, expected) in [
        (vec![], "requires"),
        (groups(&[&["P1"]]), "two or more"),
        (groups(&[&["P1", "P1"]]), "Repeated"),
        (groups(&[&["P1", "P2"], &["P2", "P3"]]), "Overlapping"),
        (groups(&[&["P1", "missing"]]), "Unknown"),
    ] {
        let error = merge_score(&input, &groups).unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn explicit_stems_pack_matching_rhythms_into_chords_and_deduplicate_unisons() {
    let input = score(&[
        ("S", vec![n("C").xml() + &n("E").xml() + &n("F").xml()]),
        ("A", vec![n("D").xml() + &n("E").extra("<notations><articulations><accent/></articulations></notations><lyric><text>word</text></lyric>").xml()
            + &n("G").xml() + &n("F").chord().xml()]),
    ]);
    let groups = groups(&[&["P1", "P2"]]);
    for stem in ["up", "down"] {
        let root =
            merge_score_with_stems(&input, &groups, &[vec![stem.into(), stem.into()]]).unwrap();
        scorewing_core::validate_score(&root).unwrap();
        let output = parse_score(root).unwrap();
        let events = &output.parts[0].measures[0].events;
        assert_eq!(events.len(), 3);
        assert!(events
            .iter()
            .all(|event| event.voice == if stem == "up" { "1" } else { "2" }));
        assert_eq!(
            pitches(&output.parts[0], 0),
            [vec!["C", "D"], vec!["E"], vec!["F", "G"]]
        );
        assert!(events
            .iter()
            .flat_map(|e| &e.notes)
            .all(|note| text(note, "stem") == stem));
        let shared = &events[1].notes[0];
        assert_eq!(text(shared, "lyric/text"), "word");
        assert!(shared
            .get_child("notations")
            .unwrap()
            .get_child("articulations")
            .unwrap()
            .get_child("accent")
            .is_some());
        assert!(events
            .iter()
            .all(|e| e.notes[0].get_child("chord").is_none()));
    }
    let opposite = parse_score(
        merge_score_with_stems(&input, &groups, &[vec!["down".into(), "up".into()]]).unwrap(),
    )
    .unwrap();
    assert_eq!(
        opposite.parts[0].measures[0]
            .events
            .iter()
            .map(|e| e.notes.len())
            .sum::<usize>(),
        7
    );
}

#[test]
fn stem_settings_require_one_valid_direction_for_each_source() {
    let input = score(&[("S", vec![n("C").xml()]), ("A", vec![n("D").xml()])]);
    for stems in [
        vec![],
        vec![vec!["up".into()]],
        vec![vec!["up".into(), "sideways".into()]],
        vec![vec!["up".into(), "down".into()], vec![]],
    ] {
        assert!(
            merge_score_with_stems(&input, &groups(&[&["P1", "P2"]]), &stems)
                .unwrap_err()
                .to_string()
                .contains("stem direction")
        );
    }
}

#[test]
fn same_stems_preserve_distinct_rhythms_ties_pitches_and_grace_repetitions() {
    let different_notes = [
        (
            n("C").duration(2).kind("half").xml(),
            n("C").xml() + &n("D").xml(),
        ),
        (
            n("C")
                .xml()
                .replace("<voice>", "<tie type=\"start\"/><voice>"),
            n("C").xml(),
        ),
        (
            n("C")
                .extra("<notations><tied type=\"start\"/></notations>")
                .xml(),
            n("C").xml(),
        ),
        (
            n("C").xml().replace("<octave>", "<alter>1</alter><octave>"),
            n("D")
                .xml()
                .replace("<octave>", "<alter>-1</alter><octave>"),
        ),
    ];
    for (index, (upper, lower)) in different_notes.into_iter().enumerate() {
        let input = score(&[("S", vec![upper]), ("A", vec![lower])]);
        let output = merge_score_with_stems(
            &input,
            &groups(&[&["P1", "P2"]]),
            &[vec!["up".into(), "up".into()]],
        )
        .unwrap();
        scorewing_core::validate_score(&output).unwrap();
        let result = parse_score(output).unwrap();
        let events = &result.parts[0].measures[0].events;
        if index == 0 {
            assert_eq!(
                events
                    .iter()
                    .map(|event| event.voice.as_str())
                    .collect::<HashSet<_>>(),
                HashSet::from(["1", "3"])
            );
            assert_eq!(events[0].duration, frac(2));
        } else {
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].voice, "1");
            assert_eq!(events[0].notes.len(), 2);
            if index == 1 {
                assert!(events[0].notes[0].get_child("tie").is_some());
            }
            if index == 2 {
                assert!(events[0].notes[0]
                    .get_child("notations")
                    .unwrap()
                    .get_child("tied")
                    .is_some());
            }
            if index == 3 {
                assert_eq!(
                    events[0]
                        .notes
                        .iter()
                        .map(|note| text(note, "pitch/step"))
                        .collect::<Vec<_>>(),
                    ["C", "D"]
                );
            }
        }
    }
    let graces = n("D").grace().xml() + &n("D").grace().xml();
    let input = score(&[
        ("S", vec![graces.clone() + &n("C").xml()]),
        ("A", vec![graces + &n("E").xml()]),
    ]);
    let output = merge_score_with_stems(
        &input,
        &groups(&[&["P1", "P2"]]),
        &[vec!["up".into(), "up".into()]],
    )
    .unwrap();
    scorewing_core::validate_score(&output).unwrap();
    let output = parse_score(output).unwrap();
    let graces: Vec<_> = output.parts[0].measures[0]
        .events
        .iter()
        .filter(|e| e.grace())
        .collect();
    assert_eq!(graces.len(), 2);
    assert!(graces.iter().all(|e| e.voice == "1"));
}

#[test]
fn reports_all_compatibility_conflicts_and_checks_remaining_measures() {
    let key = "<attributes><key><fifths>1</fifths></key></attributes>";
    let broken = n("C").voice("7").extra("<time-modification><actual-notes>3</actual-notes><normal-notes>0</normal-notes></time-modification>").xml();
    let input = score(&[
        ("Soprano", vec![n("C").xml()]),
        ("Alto", vec![key.to_owned() + &n("C").xml(), broken]),
        ("Tenor", vec![key.to_owned() + &n("C").xml()]),
    ]);
    let error = merge_score(&input, &groups(&[&["P1", "P2", "P3"]]))
        .unwrap_err()
        .to_string();
    assert!(error.contains("different numbers of measures"), "{error}");
    for part in ["Alto", "Tenor"] {
        assert!(
            error.lines().any(|line| line.contains(part)
                && line.contains("measure 1")
                && line.contains("incompatible key")),
            "{error}"
        );
    }
    assert!(
        error.lines().any(|line| line.contains("Alto")
            && line.contains("measure 2")
            && line.contains("Invalid tuplet")),
        "{error}"
    );
}

#[test]
fn opposite_stem_unisons_retain_both_groups_in_every_measure() {
    let first = n("C").xml() + &n("D").xml() + &n("E").xml() + &n("F").xml();
    let second = n("C").xml() + &n("E").xml() + &n("E").xml() + &n("F").xml();
    let result = merged(&[
        ("S", vec![first.clone(), first.clone()]),
        ("A", vec![second, first]),
    ]);
    for measure in &result.parts[0].measures {
        assert_eq!(measure.events.len(), 8);
        for voice in ["1", "2"] {
            assert_eq!(
                measure
                    .events
                    .iter()
                    .filter(|event| event.voice == voice)
                    .map(|event| event.start.clone())
                    .collect::<Vec<_>>(),
                [frac(0), frac(1), frac(2), frac(3)]
            );
        }
    }
}

#[test]
fn reports_invalid_tuplets_for_each_source_without_rewriting_notes() {
    let invalid = "<time-modification><actual-notes>3</actual-notes><normal-notes>0</normal-notes></time-modification>";
    let upper = n("C").duration(4).voice("7").extra(invalid).xml();
    let lower = n("C").duration(4).voice("9").extra(invalid).xml()
        + &backup(3)
        + &n("G").grace().voice("9").xml();
    let input = score(&[("Soprano", vec![upper]), ("Alto", vec![lower])]);
    let error = merge_score(&input, &groups(&[&["P1", "P2"]]))
        .unwrap_err()
        .to_string();
    for (part, voice) in [("Soprano", "7"), ("Alto", "9")] {
        assert!(
            error.lines().any(|line| line.contains(part)
                && line.contains(&format!("voice {voice}"))
                && line.contains("C4")
                && line.contains("Invalid tuplet")),
            "{error}"
        );
    }
    assert_eq!(error.lines().count(), 2, "{error}");
}

#[test]
fn three_sources_stems_and_common_rests() {
    let input = score(&[
        ("S", vec![n("C").rest().xml() + &n("C").xml()]),
        ("A", vec![n("C").rest().xml() + &n("D").xml()]),
        ("T", vec![n("C").rest().xml() + &n("E").xml()]),
    ]);
    let output = merge_score(&input, &groups(&[&["P1", "P2", "P3"]])).unwrap();
    scorewing_core::validate_score(&output).unwrap();
    let result = parse_score(output).unwrap();
    let events = &result.parts[0].measures[0].events;
    assert_eq!(events.len(), 3);
    assert!(events[0].notes[0].get_child("rest").is_some());
    assert_eq!(events[1].notes.len(), 2);
    assert_eq!(
        events[1..]
            .iter()
            .map(|e| text(&e.notes[0], "stem"))
            .collect::<Vec<_>>(),
        ["up", "down"]
    );
}

#[test]
fn common_rests_after_divergence_only_appear_in_voice_one() {
    let rest = n("C").rest().xml();
    let marked_rest = n("C")
        .rest()
        .extra("<notations><fermata/></notations>")
        .xml();
    let input = score(&[
        ("S", vec![n("C").xml() + &rest + &n("C").xml() + &rest]),
        (
            "A",
            vec![n("D").xml() + &marked_rest + &n("D").xml() + &rest],
        ),
        ("T", vec![n("E").xml() + &rest + &n("E").xml() + &rest]),
    ]);
    for group in [vec!["P1", "P2"], vec!["P3", "P2", "P1"]] {
        let output = merge_score(&input, &groups(&[&group])).unwrap();
        scorewing_core::validate_score(&output).unwrap();
        let result = parse_score(output).unwrap();
        let measure = &result.parts[0].measures[0];
        let rests: Vec<_> = measure
            .events
            .iter()
            .filter(|e| e.notes[0].get_child("rest").is_some())
            .collect();
        assert_eq!(rests.len(), 2);
        assert!(rests
            .iter()
            .all(|e| e.voice == "1" && e.duration == frac(1)));
        assert_eq!(rests[0].start, frac(1));
        assert_eq!(rests[1].start, frac(3));
        assert!(rests[0].notes[0]
            .get_child("notations")
            .unwrap()
            .get_child("fermata")
            .is_some());
        for voice in 1..=2 {
            let notes: Vec<_> = measure
                .events
                .iter()
                .filter(|e| e.voice == voice.to_string() && e.notes[0].get_child("rest").is_none())
                .map(|e| (e.start.clone(), e.duration.clone()))
                .collect();
            assert_eq!(notes, [(frac(0), frac(1)), (frac(2), frac(1))]);
        }
        assert_eq!(measure.length, frac(4));
    }
}

#[test]
fn subset_rest_duplicates_collapse_but_different_durations_remain() {
    let rest = n("C").rest().xml();
    let input = score(&[
        ("S", vec![n("C").xml() + &rest + &rest + &rest]),
        (
            "A",
            vec![n("D").xml() + &rest + &n("C").rest().duration(2).kind("half").xml()],
        ),
        ("T", vec![n("E").xml() + &n("E").xml() + &rest + &rest]),
    ]);
    let output = merge_score(&input, &groups(&[&["P1", "P2", "P3"]])).unwrap();
    scorewing_core::validate_score(&output).unwrap();
    let result = parse_score(output).unwrap();
    let rests: Vec<_> = result.parts[0].measures[0]
        .events
        .iter()
        .filter(|e| e.notes[0].get_child("rest").is_some())
        .map(|e| (e.voice.as_str(), e.start.clone(), e.duration.clone()))
        .collect();
    assert_eq!(
        rests,
        [
            ("1", frac(1), frac(1)),
            ("1", frac(2), frac(1)),
            ("1", frac(3), frac(1)),
            ("2", frac(2), frac(2)),
        ]
    );
}

#[test]
fn source_polyphony_compacts_chords_and_allocates_independent_rhythms() {
    let chord = n("C").duration(2).kind("half").xml()
        + &backup(2)
        + &n("E").duration(2).voice("7").kind("half").xml();
    let result = merged(&[
        ("S", vec![chord]),
        ("A", vec![n("G").duration(2).kind("half").xml()]),
    ]);
    assert_eq!(
        result.parts[0].measures[0]
            .events
            .iter()
            .map(|e| e.notes.len())
            .collect::<Vec<_>>(),
        [2, 1]
    );
    let same = n("C").xml() + &backup(1) + &n("C").voice("2").xml();
    let result = merged(&[("S", vec![same]), ("A", vec![n("C").xml()])]);
    assert_eq!(result.parts[0].measures[0].events.len(), 2);
    assert!(result.parts[0].measures[0]
        .events
        .iter()
        .all(|event| event.notes.len() == 1));
    let independent = n("C").duration(2).kind("half").xml()
        + &backup(2)
        + &n("E").voice("2").xml()
        + &n("F").voice("2").xml();
    let result = merged(&[
        ("S", vec![independent]),
        ("A", vec![n("C").duration(2).kind("half").xml()]),
    ]);
    let events = &result.parts[0].measures[0].events;
    assert_eq!(
        events
            .iter()
            .map(|e| (e.voice.as_str(), e.start.clone(), e.duration.clone()))
            .collect::<Vec<_>>(),
        [
            ("1", frac(0), frac(2)),
            ("2", frac(0), frac(2)),
            ("3", frac(0), frac(1)),
            ("3", frac(1), frac(1))
        ]
    );
}

#[test]
fn rhythms_spelled_pitches_and_sound_or_visual_tie_differences_do_not_collapse() {
    let result = merged(&[
        ("S", vec![n("C").duration(2).kind("half").xml()]),
        ("A", vec![n("C").xml() + &n("D").xml()]),
    ]);
    assert_eq!(
        result.parts[0].measures[0]
            .events
            .iter()
            .map(|e| e.voice.as_str())
            .collect::<HashSet<_>>(),
        HashSet::from(["1", "2"])
    );
    let sharp = n("C").xml().replace("<octave>", "<alter>1</alter><octave>");
    let flat = n("D")
        .xml()
        .replace("<octave>", "<alter>-1</alter><octave>");
    assert_eq!(
        merged(&[("S", vec![sharp]), ("A", vec![flat])]).parts[0].measures[0]
            .events
            .len(),
        2
    );
    for tied in [
        n("C")
            .xml()
            .replace("<voice>", "<tie type=\"start\"/><voice>"),
        n("C")
            .extra("<notations><tied type=\"start\"/></notations>")
            .xml(),
    ] {
        let result = merged(&[("S", vec![tied]), ("A", vec![n("C").xml()])]);
        assert_eq!(result.parts[0].measures[0].events.len(), 2);
        assert_eq!(
            result.parts[0].measures[0]
                .events
                .iter()
                .map(|e| e.voice.as_str())
                .collect::<HashSet<_>>(),
            HashSet::from(["1", "2"])
        );
    }
}

#[test]
fn differing_graces_preserve_sustained_note_values_and_unique_identifiers() {
    let held = n("C")
        .duration(4)
        .kind("whole")
        .xml()
        .replace("<note>", "<note id=\"held\">");
    let at_two = backup(2) + &n("D").grace().xml() + &forward(2);
    let result = merged(&[("S", vec![held.clone() + &at_two]), ("A", vec![held])]);
    let ordinary: Vec<_> = result.parts[0].measures[0]
        .events
        .iter()
        .filter(|event| !event.grace())
        .collect();
    assert_eq!(
        ordinary
            .iter()
            .map(|e| (e.start.clone(), e.duration.clone(), e.voice.as_str()))
            .collect::<Vec<_>>(),
        [(frac(0), frac(4), "1"), (frac(0), frac(4), "2")]
    );
    assert!(ordinary
        .iter()
        .all(|event| text(&event.notes[0], "type") == "whole"
            && event.notes[0].get_child("tie").is_none()));
    let grace = result.parts[0].measures[0]
        .events
        .iter()
        .find(|event| event.grace())
        .unwrap();
    assert_eq!(grace.start, frac(2));
    let ids = ids(&result.parts[0].source);
    assert_eq!(ids.len(), ids.iter().collect::<HashSet<_>>().len());
    let grace = n("D").grace().xml() + &n("E").grace().xml();
    let result = merged(&[
        ("S", vec![grace.clone() + &n("C").xml()]),
        ("A", vec![grace + &n("C").xml()]),
    ]);
    assert_eq!(
        result.parts[0].measures[0]
            .events
            .iter()
            .map(|e| (text(&e.notes[0], "pitch/step"), e.grace()))
            .collect::<Vec<_>>(),
        [
            ("D".into(), true),
            ("E".into(), true),
            ("C".into(), false),
            ("D".into(), true),
            ("E".into(), true),
            ("C".into(), false)
        ]
    );
}

#[test]
fn lyric_lines_follow_stems_and_identical_sung_content_is_deduplicated() {
    let lyric = |text: &str| format!("<lyric number=\"1\"><text>{text}</text></lyric>");
    let upper = n("C").extra(&lyric("hello")).xml() + &n("D").extra(&lyric("same")).xml();
    let lower = n("E").extra(&lyric("world")).xml() + &n("F").extra(&lyric("same")).xml();
    let input = score(&[("Lower", vec![lower]), ("Upper", vec![upper])]);
    let output = merge_score_with_stems(
        &input,
        &groups(&[&["P1", "P2"]]),
        &[vec!["down".into(), "up".into()]],
    )
    .unwrap();
    scorewing_core::validate_score(&output).unwrap();
    let result = parse_score(output).unwrap();
    let lyrics: Vec<_> = result.parts[0].measures[0]
        .events
        .iter()
        .flat_map(|event| {
            event
                .notes
                .iter()
                .flat_map(elements)
                .filter(|node| node.name == "lyric")
                .map(move |lyric| {
                    (
                        event.voice.as_str(),
                        event.start.clone(),
                        lyric.attributes["number"].as_str(),
                        text(lyric, "text"),
                        lyric.attributes["placement"].as_str(),
                    )
                })
        })
        .collect();
    assert_eq!(
        lyrics,
        [
            ("1", frac(0), "1", "hello".into(), "below"),
            ("1", frac(1), "1", "same".into(), "below"),
            ("2", frac(0), "2", "world".into(), "below")
        ]
    );
}

#[test]
fn annotations_directions_clef_stemless_and_metadata() {
    let direction = "<direction><direction-type><words>quiet</words></direction-type></direction>";
    let marks = "<notations><articulations><staccato/></articulations></notations>";
    let other = "<notations><articulations><accent/></articulations></notations>";
    let result = merged(&[
        (
            "S",
            vec![
                direction.to_owned()
                    + &n("C")
                        .octave(3)
                        .duration(4)
                        .kind("whole")
                        .extra(marks)
                        .xml(),
            ],
        ),
        (
            "A",
            vec![
                direction.to_owned()
                    + &n("C")
                        .octave(3)
                        .duration(4)
                        .kind("whole")
                        .extra(other)
                        .xml(),
            ],
        ),
    ]);
    let measure = &result.parts[0].measures[0];
    let clef = descendants(&measure.source)
        .into_iter()
        .find(|e| e.name == "clef")
        .unwrap();
    assert_eq!(text(clef, "sign"), "F");
    let directions: Vec<_> = elements(&measure.source)
        .filter(|e| e.name == "direction")
        .collect();
    assert_eq!(directions.len(), 1);
    assert_eq!(text(directions[0], "voice"), "1");
    assert!(measure
        .events
        .iter()
        .all(|event| event.notes[0].get_child("stem").is_none()));
    assert_eq!(
        measure
            .events
            .iter()
            .flat_map(|event| descendants(&event.notes[0]))
            .filter(|node| ["staccato", "accent"].contains(&node.name.as_str()))
            .count(),
        2
    );
    assert_eq!(text(&result.root, "work/work-title"), "Choir fixture");
}

#[test]
fn compatibility_tracks_mid_measure_changes_and_boundaries() {
    for (change, label) in [
        (
            "<time><beats>3</beats><beat-type>4</beat-type></time>",
            "time",
        ),
        ("<key><fifths>2</fifths></key>", "key"),
        (
            "<transpose><diatonic>1</diatonic><chromatic>2</chromatic></transpose>",
            "transpose",
        ),
    ] {
        let altered = n("C").xml() + "<attributes>" + change + "</attributes>" + &n("C").xml();
        let input = score(&[
            ("S", vec![n("C").xml() + &n("C").xml()]),
            ("A", vec![altered]),
        ]);
        let error = merge_score(&input, &groups(&[&["P1", "P2"]]))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(&format!("position 1: incompatible {label}")),
            "{error}"
        );
    }
    let input = score(&[
        ("S", vec![n("C").duration(2).xml()]),
        ("A", vec![n("C").xml()]),
    ]);
    assert!(merge_score(&input, &groups(&[&["P1", "P2"]]))
        .unwrap_err()
        .to_string()
        .contains("incompatible measure boundaries"));
    let input = score(&[
        ("S", vec![n("C").xml()]),
        ("A", vec![n("C").xml(), n("D").xml()]),
    ]);
    assert!(merge_score(&input, &groups(&[&["P1", "P2"]]))
        .unwrap_err()
        .to_string()
        .contains("different numbers of measures"));
}

#[test]
fn exact_changing_divisions_and_hidden_signatures_zero_transpose() {
    let tuplet="<time-modification><actual-notes>3</actual-notes><normal-notes>1</normal-notes></time-modification>";
    let upper = n("C").xml()
        + "<attributes><divisions>3</divisions></attributes>"
        + &n("D").extra(tuplet).xml();
    let lower = n("C").duration(3).xml() + &n("D").extra(tuplet).xml();
    let mut input = score(&[("S", vec![upper]), ("A", vec![lower])]);
    let second = input
        .root
        .children
        .iter_mut()
        .filter_map(xmltree::XMLNode::as_mut_element)
        .filter(|e| e.name == "part")
        .nth(1)
        .unwrap();
    scorewing_core::xml::set_child(
        second
            .get_mut_child("measure")
            .unwrap()
            .get_mut_child("attributes")
            .unwrap(),
        "divisions",
        "3",
    );
    input = parse_score(input.root).unwrap();
    let output = merge_score(&input, &groups(&[&["P1", "P2"]])).unwrap();
    scorewing_core::validate_score(&output).unwrap();
    let result = parse_score(output).unwrap();
    assert_eq!(
        result.parts[0].measures[0]
            .events
            .iter()
            .map(|e| (e.start.clone(), e.duration.clone()))
            .collect::<Vec<_>>(),
        [
            (frac(0), frac(1)),
            (frac(1), frac(1) / frac(3)),
            (frac(0), frac(1)),
            (frac(1), frac(1) / frac(3))
        ]
    );
    let mut input = score(&[("S", vec![n("C").xml()]), ("A", vec![n("D").xml()])]);
    let second = input
        .root
        .children
        .iter_mut()
        .filter_map(xmltree::XMLNode::as_mut_element)
        .filter(|e| e.name == "part")
        .nth(1)
        .unwrap();
    let attrs = second
        .get_mut_child("measure")
        .unwrap()
        .get_mut_child("attributes")
        .unwrap();
    for tag in ["time", "key"] {
        let node = attrs.get_mut_child(tag).unwrap();
        node.attributes.insert("print-object".into(), "no".into());
        node.attributes.insert("id".into(), format!("hidden-{tag}"));
        node.attributes.insert("default-y".into(), "20".into());
    }
    append(attrs,element("<transpose><diatonic>0</diatonic><chromatic>0</chromatic><octave-change>0</octave-change></transpose>"));
    let input = parse_score(input.root).unwrap();
    let output = merge_score(&input, &groups(&[&["P1", "P2"]])).unwrap();
    scorewing_core::validate_score(&output).unwrap();
    assert_eq!(
        parse_score(output).unwrap().parts[0].measures[0]
            .events
            .len(),
        2
    );
}

#[test]
fn relationships_remain_distinct_across_stem_groups() {
    let start = "<notations><slur type=\"start\" number=\"1\"/></notations>";
    let stop = "<notations><slur type=\"stop\" number=\"1\"/></notations>";
    let upper = n("C").extra(start).xml() + &n("D").extra(stop).xml();
    let lower = n("C").extra(start).xml() + &n("E").extra(stop).xml();
    let result = merged(&[("S", vec![upper]), ("A", vec![lower])]);
    assert_eq!(
        descendants(&result.parts[0].source)
            .into_iter()
            .filter(|n| n.name == "slur")
            .map(|n| (
                n.attributes["type"].as_str(),
                n.attributes["number"].as_str()
            ))
            .collect::<Vec<_>>(),
        [("start", "1"), ("stop", "1"), ("start", "2"), ("stop", "2")]
    );
}

#[test]
fn relationship_number_overflow_is_rejected_instead_of_losing_associations() {
    let slurs: String = (1..=16)
        .map(|number| format!("<slur type=\"start\" number=\"{number}\"/>"))
        .collect();
    let upper = n("C")
        .extra(&format!("<notations>{slurs}</notations>"))
        .xml();
    let lower = n("D")
        .extra("<notations><slur type=\"start\" number=\"1\"/></notations>")
        .xml();
    let input = score(&[("S", vec![upper]), ("A", vec![lower])]);
    assert!(merge_score(&input, &groups(&[&["P1", "P2"]]))
        .unwrap_err()
        .to_string()
        .contains("too many distinct slur relationships"));
}

#[test]
fn names_instruments_players_lyrics_play_and_overrides_survive() {
    let shared = merged(&[
        (
            "S",
            vec![n("C").extra("<play><mute>off</mute></play>").xml()],
        ),
        (
            "A",
            vec![n("C")
                .extra("<lyric number=\"1\"><text>word</text></lyric>")
                .xml()],
        ),
    ]);
    let note = &shared.parts[0].measures[0].events[0].notes[0];
    assert_eq!(
        text(
            &shared.parts[0].measures[0].events[1].notes[0],
            "lyric/text"
        ),
        "word"
    );
    assert!(note.get_child("play").is_some());
    for shared_prefix in [false, true] {
        let mut input = score(&[
            (
                "S",
                vec![n("C").extra("<play><mute>off</mute></play>").xml() + &n("E").xml()],
            ),
            (
                "A",
                vec![
                    n(if shared_prefix { "C" } else { "D" })
                        .extra("<lyric number=\"1\"><text>word</text></lyric>")
                        .xml()
                        + &n("F").xml(),
                ],
            ),
        ]);
        for (source, part) in input.parts.iter_mut().enumerate() {
            let index = source + 1;
            append(
                &mut part.definition,
                element(
                    "<part-name-display><display-text>OLD NAME</display-text></part-name-display>",
                ),
            );
            append(&mut part.definition,element(&format!("<score-instrument id=\"I{index}\"><instrument-name>Choir</instrument-name></score-instrument>")));
            append(
                &mut part.definition,
                element(&format!(
                    "<player id=\"Player{index}\"><player-name>Singer</player-name></player>"
                )),
            );
            append(&mut part.definition,element(&format!("<midi-instrument id=\"I{index}\"><midi-channel>{index}</midi-channel></midi-instrument>")));
            if shared_prefix {
                let note = &mut part.measures[0].events[0].notes[0];
                scorewing_core::xml::insert_note_child(
                    note,
                    element(&format!("<instrument id=\"I{index}\"/>")),
                );
                note.attributes.insert("id".into(), format!("N{index}"));
            }
        }
        append(&mut input.parts[1].definition,element("<score-instrument id=\"AltOverride\"><instrument-name>Alternate alto</instrument-name></score-instrument>"));
        scorewing_core::xml::insert_note_child(
            &mut input.parts[1].measures[0].events[1].notes[0],
            element("<instrument id=\"AltOverride\"/>"),
        );
        let output = merge_score(&input, &groups(&[&["P1", "P2"]])).unwrap();
        scorewing_core::validate_score(&output).unwrap();
        let result = parse_score(output).unwrap();
        let part = &result.parts[0];
        assert!(part.definition.get_child("part-name-display").is_none());
        assert_eq!(
            elements(&part.definition)
                .filter(|p| p.name == "player")
                .map(|p| p.attributes["id"].as_str())
                .collect::<Vec<_>>(),
            ["Player1", "Player2"]
        );
        assert_eq!(
            elements(&part.definition)
                .filter(|p| p.name == "midi-instrument")
                .map(|p| p.attributes["id"].as_str())
                .collect::<Vec<_>>(),
            ["I1", "I2"]
        );
        let events = &part.measures[0].events;
        assert_eq!(
            events
                .iter()
                .map(|e| (
                    e.voice.as_str(),
                    e.notes[0].get_child("instrument").unwrap().attributes["id"].as_str()
                ))
                .collect::<Vec<_>>(),
            vec![("1", "I1"), ("1", "I1"), ("2", "I2"), ("2", "AltOverride")]
        );
        assert!(events[0].notes[0].get_child("play").is_some());
        assert_eq!(text(&events[2].notes[0], "lyric/text"), "word");
    }
}
