mod common;
use common::*;
use mxml_core::{
    model::{frac, parse_score},
    split::split_score,
};
use std::collections::HashSet;

#[test]
fn reports_relationship_errors_across_parts_measures_and_voices() {
    let broken = |step: &str, voice: &str| {
        n(step)
            .voice(voice)
            .extra(
                "<tie type=\"start\"/><notations><slur type=\"start\" number=\"1\"/></notations>",
            )
            .xml()
            + &n("G").octave(3).voice(voice).chord().xml()
            + &n(step)
                .voice(voice)
                .extra(
                    "<tie type=\"stop\"/><notations><slur type=\"stop\" number=\"1\"/></notations>",
                )
                .xml()
            + &n("B").octave(5).voice(voice).chord().xml()
    };
    let original = score(&[
        ("Soprano", vec![broken("C", "1"), broken("D", "1")]),
        ("Alto", vec![broken("E", "2"), broken("F", "2")]),
    ]);
    let error = split_score(&original, "chords", None)
        .unwrap_err()
        .to_string();
    for (part, voice, notes) in [("Soprano", "1", ["C4", "D4"]), ("Alto", "2", ["E4", "F4"])] {
        for (index, note) in notes.iter().enumerate() {
            for kind in ["ambiguous tie", "ambiguous slur"] {
                assert!(
                    error.lines().any(|line| line.contains(part)
                        && line.contains(&format!("measure {}", index + 1))
                        && line.contains(&format!("voice {voice}"))
                        && line.contains(note)
                        && line.contains(kind)),
                    "{error}"
                );
            }
        }
    }
    let selected = split_score(&original, "chords", Some(&["P1".into()]))
        .unwrap_err()
        .to_string();
    assert!(!selected.contains("Alto"), "{selected}");
    let single_measure = score(&[("Soprano", vec![broken("C", "1") + &broken("D", "1")])]);
    let error = split_score(&single_measure, "chords", None)
        .unwrap_err()
        .to_string();
    for position in [1, 3] {
        for kind in ["ambiguous tie", "ambiguous slur"] {
            assert!(
                error
                    .lines()
                    .any(|line| line.contains(&format!("position {position},"))
                        && line.contains(kind)),
                "{error}"
            );
        }
    }
}

#[test]
fn late_divisi_retains_original_order_metadata_and_sources() {
    let original = score(&[
        (
            "Soprano",
            vec![n("G").xml(), n("C").xml() + &n("G").chord().xml()],
        ),
        ("Alto", vec![n("E").xml(), n("F").xml()]),
    ]);
    let result = parse_score(split_score(&original, "chords", None).unwrap()).unwrap();
    assert_eq!(
        names(&result),
        ["Soprano", "Soprano 1", "Soprano 2", "Alto"]
    );
    assert_eq!(result.parts[0].source, original.parts[0].source);
    assert_eq!(result.parts[3].source, original.parts[1].source);
    assert_eq!(pitches(&result.parts[1], 0), [["G"]]);
    assert_eq!(pitches(&result.parts[2], 0), [["G"]]);
    assert_eq!(pitches(&result.parts[1], 1), [["G"]]);
    assert_eq!(pitches(&result.parts[2], 1), [["C"]]);
    assert_eq!(text(&result.root, "work/work-title"), "Choir fixture");
}

#[test]
fn monophonic_unchanged_voice_mode_keeps_chords_and_numeric_voice_order() {
    let body = n("C").xml() + &n("E").chord().xml();
    let mono = score(&[("S", vec![body.clone()])]);
    assert_eq!(split_score(&mono, "voices", None).unwrap(), mono.root);
    let poly = score(&[("S", vec![body + &backup(1) + &n("G").voice("2").xml()])]);
    let result = parse_score(split_score(&poly, "voices", None).unwrap()).unwrap();
    assert_eq!(pitches(&result.parts[1], 0), [["C", "E"]]);
    assert_eq!(pitches(&result.parts[2], 0), [["G"]]);
    assert!(result.parts[1..]
        .iter()
        .flat_map(|p| &p.measures)
        .flat_map(|m| &m.events)
        .all(|e| e.voice == "1"));
    let mut body = String::new();
    for (index, (voice, step)) in [("z", "B"), ("10", "G"), ("2", "E"), ("a", "C")]
        .iter()
        .enumerate()
    {
        if index > 0 {
            body += &backup(1);
        }
        body += &n(step).voice(voice).xml();
    }
    let result =
        parse_score(split_score(&score(&[("Choir", vec![body])]), "chords", None).unwrap())
            .unwrap();
    assert_eq!(
        result.parts[1..]
            .iter()
            .map(|p| pitches(p, 0)[0][0].clone())
            .collect::<Vec<_>>(),
        ["E", "G", "C", "B"]
    );
}

#[test]
fn partial_chords_rests_and_shared_single_notes() {
    let body = n("C").xml()
        + &n("E").chord().xml()
        + &n("G").chord().xml()
        + &n("D").xml()
        + &n("F").chord().xml()
        + &n("A").xml()
        + &n("C").rest().xml();
    let result =
        parse_score(split_score(&score(&[("S", vec![body])]), "chords", None).unwrap()).unwrap();
    assert_eq!(
        pitches(&result.parts[1], 0),
        [["G"], ["F"], ["A"], ["rest"]]
    );
    assert_eq!(
        pitches(&result.parts[2], 0),
        [["E"], ["D"], ["A"], ["rest"]]
    );
    assert_eq!(
        pitches(&result.parts[3], 0),
        [["C"], ["rest"], ["A"], ["rest"]]
    );
    let body = n("C").xml()
        + &n("D").xml()
        + &backup(1)
        + &n("C").rest().voice("2").xml()
        + &n("E").voice("2").xml();
    let result =
        parse_score(split_score(&score(&[("S", vec![body])]), "chords", None).unwrap()).unwrap();
    assert_eq!(pitches(&result.parts[1], 0), [["C"], ["D"], ["E"]]);
    assert_eq!(pitches(&result.parts[2], 0), [["C"], ["rest"], ["E"]]);
}

#[test]
fn mixed_explicit_voices_and_chords() {
    let body = n("C").xml() + &n("E").chord().xml() + &backup(1) + &n("G").voice("2").xml();
    let result =
        parse_score(split_score(&score(&[("S", vec![body])]), "chords", None).unwrap()).unwrap();
    assert_eq!(
        result.parts[1..]
            .iter()
            .map(|p| pitches(p, 0)[0][0].clone())
            .collect::<Vec<_>>(),
        ["E", "C", "G"]
    );
}

#[test]
fn directions_follow_their_explicit_voice_and_are_renumbered() {
    let direction="<direction><direction-type><words>Lower only</words></direction-type><voice>2</voice></direction>";
    let body = direction.to_owned() + &n("C").xml() + &backup(1) + &n("G").voice("2").xml();
    let result =
        parse_score(split_score(&score(&[("S", vec![body])]), "voices", None).unwrap()).unwrap();
    assert!(!elements(&result.parts[1].measures[0].source).any(|e| e.name == "direction"));
    let direction = elements(&result.parts[2].measures[0].source)
        .find(|e| e.name == "direction")
        .unwrap();
    assert_eq!(text(direction, "voice"), "1");
    assert_eq!(text(direction, "direction-type/words"), "Lower only");
}

#[test]
fn pickups_silence_changing_divisions_tuplets_and_graces() {
    let body = forward(1) + &n("C").xml() + &n("G").chord().xml();
    let result = parse_score(
        split_score(
            &score_div(&[("S", vec![body, String::new(), n("E").xml()])], 3),
            "chords",
            None,
        )
        .unwrap(),
    )
    .unwrap();
    for part in &result.parts[1..] {
        assert_eq!(
            part.measures
                .iter()
                .map(|m| m.length.clone())
                .collect::<Vec<_>>(),
            [frac(2) / frac(3), frac(4), frac(1) / frac(3)]
        );
        assert!(part.measures[0].events[0].notes[0]
            .get_child("rest")
            .is_some());
        assert_eq!(pitches(part, 1), [["rest"]]);
    }
    let tuplet="<time-modification><actual-notes>3</actual-notes><normal-notes>2</normal-notes></time-modification>";
    let body = n("D").grace().xml()
        + &n("F").grace().chord().xml()
        + &n("C").duration(2).extra(tuplet).xml()
        + &n("G").duration(2).chord().extra(tuplet).xml()
        + "<attributes><divisions>6</divisions></attributes>"
        + &n("E").duration(6).xml();
    let result =
        parse_score(split_score(&score_div(&[("S", vec![body])], 3), "chords", None).unwrap())
            .unwrap();
    for part in &result.parts[1..] {
        assert_eq!(part.measures[0].length, frac(5) / frac(3));
        assert!(part.measures[0].events[0].grace());
        assert!(part.measures[0].events[1].notes[0]
            .get_child("time-modification")
            .is_some());
    }
}

#[test]
fn missing_second_voice_inherits_first_voice_even_with_other_voices() {
    let first = n("G").xml() + &backup(1) + &n("F").voice("2").xml();
    let shared = n("A").xml() + &backup(1) + &n("C").voice("3").xml();
    let explicit_rest = n("B").xml() + &backup(1) + &n("C").voice("2").rest().xml();
    let original = score(&[("Alt", vec![first, shared, explicit_rest, n("D").xml()])]);
    for mode in ["chords", "voices"] {
        let result = parse_score(split_score(&original, mode, None).unwrap()).unwrap();
        assert_eq!(result.parts[0].source, original.parts[0].source);
        for (measure, expected) in ["F", "A", "rest", "D"].iter().enumerate() {
            assert_eq!(pitches(&result.parts[2], measure), [[*expected]]);
        }
    }
}

#[test]
fn shared_voice_transition_repairs_ties_and_inserts_a_slur_for_changed_pitch() {
    let start = "<tie type=\"start\"/><notations><tied type=\"start\"/></notations>";
    let stop = "<tie type=\"stop\"/><notations><tied type=\"stop\"/></notations>";
    for lower_pitch in ["F", "G"] {
        let first = n("G").extra(start).xml() + &backup(1) + &n(lower_pitch).voice("2").xml();
        let original = score(&[("Alt", vec![first, n("G").extra(stop).xml()])]);
        for mode in ["chords", "voices"] {
            let result = parse_score(split_score(&original, mode, None).unwrap()).unwrap();
            assert_eq!(result.parts[0].source, original.parts[0].source);
            let upper = &result.parts[1];
            assert_eq!(
                upper.measures[0].events[0].notes[0]
                    .get_child("tie")
                    .unwrap()
                    .attributes["type"],
                "start"
            );
            assert_eq!(
                upper.measures[1].events[0].notes[0]
                    .get_child("tie")
                    .unwrap()
                    .attributes["type"],
                "stop"
            );
            let lower = &result.parts[2];
            assert_eq!(pitches(lower, 0), [[lower_pitch]]);
            assert_eq!(pitches(lower, 1), [["G"]]);
            let left = &lower.measures[0].events[0].notes[0];
            let right = &lower.measures[1].events[0].notes[0];
            let tag = if lower_pitch == "G" { "tied" } else { "slur" };
            let left_mark = left.get_child("notations").unwrap().get_child(tag).unwrap();
            let right_mark = right
                .get_child("notations")
                .unwrap()
                .get_child(tag)
                .unwrap();
            assert_eq!(left_mark.attributes["type"], "start");
            assert_eq!(right_mark.attributes["type"], "stop");
            if tag == "tied" {
                assert_eq!(left.get_child("tie").unwrap().attributes["type"], "start");
                assert_eq!(right.get_child("tie").unwrap().attributes["type"], "stop");
            }
            if tag == "slur" {
                assert_eq!(
                    left_mark.attributes["number"],
                    right_mark.attributes["number"]
                );
                assert!(left.get_child("tie").is_none());
                assert!(right.get_child("tie").is_none());
                assert!(right
                    .get_child("notations")
                    .unwrap()
                    .get_child("tied")
                    .is_none());
            }
        }
    }
}

#[test]
fn shared_ties_connect_to_returning_voices_without_slur_number_collisions() {
    let start = "<tie type=\"start\"/><notations><tied type=\"start\"/><slur type=\"start\" number=\"1\"/></notations>";
    let stop = "<tie type=\"stop\"/><notations><tied type=\"stop\"/></notations>";
    let close = "<notations><slur type=\"stop\" number=\"1\"/></notations>";
    for same_measure in [false, true] {
        let shared = n("G").extra(start).xml();
        let divisi = n("G").extra(stop).xml() + &backup(1) + &n("F").voice("2").xml();
        let measures = if same_measure {
            vec![shared + &divisi, n("A").extra(close).xml()]
        } else {
            vec![shared, divisi, n("A").extra(close).xml()]
        };
        let original = score(&[("Alt", measures)]);
        for mode in ["chords", "voices"] {
            // The unrelated source slur changes lane assignment on its middle note only.
            let result = parse_score(split_score(&original, mode, None).unwrap()).unwrap();
            let lower = &result.parts[2];
            let left = &lower.measures[0].events[0].notes[0];
            let right = if same_measure {
                &lower.measures[0].events[1].notes[0]
            } else {
                &lower.measures[1].events[0].notes[0]
            };
            let generated = elements(left.get_child("notations").unwrap())
                .find(|e| e.name == "slur" && e.attributes["number"] != "1")
                .unwrap();
            let stop = right
                .get_child("notations")
                .unwrap()
                .get_child("slur")
                .unwrap();
            assert_eq!(generated.attributes["type"], "start");
            assert_eq!(stop.attributes["type"], "stop");
            assert_eq!(generated.attributes["number"], stop.attributes["number"]);
            assert!(left.get_child("tie").is_none());
            assert!(right.get_child("tie").is_none());
        }
    }
}

#[test]
fn shared_voice_rest_and_untied_transitions_do_not_gain_connections() {
    let stop = "<tie type=\"stop\"/><notations><tied type=\"stop\"/></notations>";
    for rest in [false, true] {
        let lower = if rest { n("F").rest() } else { n("F") }.voice("2").xml();
        let first = n("G").xml() + &backup(1) + &lower;
        let shared = if rest { n("G").extra(stop) } else { n("G") }.xml();
        let original = score(&[("Alt", vec![first, shared])]);
        for mode in ["chords", "voices"] {
            let result = parse_score(split_score(&original, mode, None).unwrap()).unwrap();
            assert!(!descendants(&result.parts[2].source)
                .iter()
                .any(|e| ["tie", "tied", "slur"].contains(&e.name.as_str())));
        }
    }
}

#[test]
fn shared_chord_ties_connect_matching_pitches_in_voice_mode() {
    let start = "<tie type=\"start\"/><notations><tied type=\"start\"/></notations>";
    let stop = "<tie type=\"stop\"/><notations><tied type=\"stop\"/></notations>";
    let first = n("C").extra(start).xml()
        + &n("E").chord().extra(start).xml()
        + &backup(1)
        + &n("C").voice("2").xml()
        + &n("E").voice("2").chord().xml();
    let second = n("C").extra(stop).xml() + &n("E").chord().extra(stop).xml();
    let result =
        parse_score(split_score(&score(&[("Alt", vec![first, second])]), "voices", None).unwrap())
            .unwrap();
    for (measure, kind) in [(0, "start"), (1, "stop")] {
        for note in &result.parts[2].measures[measure].events[0].notes {
            assert_eq!(note.get_child("tie").unwrap().attributes["type"], kind);
            assert_eq!(
                note.get_child("notations")
                    .unwrap()
                    .get_child("tied")
                    .unwrap()
                    .attributes["type"],
                kind
            );
        }
    }
}

#[test]
fn relationships_preserved_or_ambiguity_rejected() {
    let start = "<tie type=\"start\"/><notations><tied type=\"start\"/></notations>";
    let stop = "<tie type=\"stop\"/><notations><tied type=\"stop\"/></notations>";
    let first = n("C").extra(start).xml() + &n("G").chord().xml();
    let second = n("C").extra(stop).xml() + &n("G").chord().xml();
    let result = parse_score(
        split_score(
            &score(&[("S", vec![first, second.clone()])]),
            "chords",
            None,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        result.parts[2].measures[0].events[0].notes[0]
            .get_child("tie")
            .unwrap()
            .attributes["type"],
        "start"
    );
    assert_eq!(
        result.parts[2].measures[1].events[0].notes[0]
            .get_child("tie")
            .unwrap()
            .attributes["type"],
        "stop"
    );
    let error = split_score(
        &score(&[("S", vec![n("C").extra(start).xml(), second])]),
        "chords",
        None,
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("measure 2, position 0") && error.contains("ambiguous tie"),
        "{error}"
    );
    let first = n("C")
        .extra("<notations><slur type=\"start\" number=\"1\"/></notations>")
        .xml();
    let second = n("C")
        .extra("<notations><slur type=\"stop\" number=\"1\"/></notations>")
        .xml()
        + &n("G").chord().xml();
    assert!(
        split_score(&score(&[("S", vec![first, second])]), "chords", None)
            .unwrap_err()
            .to_string()
            .contains("ambiguous slur")
    );
}

#[test]
fn chord_marks_lyrics_instruments_players_and_unique_ids() {
    let marks="<time-modification><actual-notes>3</actual-notes><normal-notes>2</normal-notes></time-modification><notations><articulations><staccato/></articulations><tuplet type=\"start\"/></notations><lyric number=\"1\"><text>Sing</text></lyric>";
    let body = n("C").duration(2).extra(marks).xml() + &n("G").duration(2).chord().xml();
    let mut original = score_div(&[("S", vec![body])], 3);
    let part = &mut original.parts[0];
    append(
        &mut part.definition,
        element("<part-name-display><display-text>Old</display-text></part-name-display>"),
    );
    append(&mut part.definition,element("<score-instrument id=\"I1\"><instrument-name>Choir</instrument-name></score-instrument>"));
    append(
        &mut part.definition,
        element("<player id=\"Singer\"><player-name>Singer</player-name></player>"),
    );
    for (index, note) in part.measures[0].events[0].notes.iter_mut().enumerate() {
        note.attributes.insert("id".into(), format!("N{index}"));
        note.attributes.insert("default-x".into(), "42".into());
        mxml_core::xml::insert_note_child(note, element("<instrument id=\"I1\"/>"));
        append(
            note,
            element("<listen><assess type=\"yes\" player=\"Singer\"/></listen>"),
        );
    }
    let before = original.root.clone();
    let result = parse_score(split_score(&original, "chords", None).unwrap()).unwrap();
    assert_eq!(original.root, before);
    for part in &result.parts[1..] {
        assert!(part.definition.get_child("part-name-display").is_none());
        let note = &part.measures[0].events[0].notes[0];
        assert_eq!(
            note.get_child("instrument").unwrap().attributes["id"],
            part.definition
                .get_child("score-instrument")
                .unwrap()
                .attributes["id"]
        );
        assert_eq!(
            note.get_child("listen")
                .unwrap()
                .get_child("assess")
                .unwrap()
                .attributes["player"],
            part.definition.get_child("player").unwrap().attributes["id"]
        );
        assert!(!note.attributes.contains_key("default-x"));
        assert_eq!(text(note, "lyric/text"), "Sing");
        assert_eq!(text(note, "time-modification/actual-notes"), "3");
        assert!(note
            .get_child("notations")
            .unwrap()
            .get_child("articulations")
            .unwrap()
            .get_child("staccato")
            .is_some());
        assert_eq!(
            note.get_child("notations")
                .unwrap()
                .get_child("tuplet")
                .unwrap()
                .attributes["type"],
            "start"
        );
        let ids = ids(&part.source);
        assert_eq!(ids.len(), ids.iter().collect::<HashSet<_>>().len());
    }
}

#[test]
fn shared_note_slicing_preserves_timing_without_dangling_added_ties_or_duplicate_ids() {
    let body = n("C").duration(2).kind("half").xml() + &backup(1) + &n("G").voice("2").xml();
    let result =
        parse_score(split_score(&score(&[("S", vec![body])]), "chords", None).unwrap()).unwrap();
    assert_eq!(result.parts[1].measures[0].events.len(), 1);
    assert_eq!(
        result.parts[2].measures[0]
            .events
            .iter()
            .map(|e| (e.start.clone(), e.duration.clone()))
            .collect::<Vec<_>>(),
        [(frac(0), frac(1)), (frac(1), frac(1))]
    );
    assert!(result.parts[2].measures[0].events[0].notes[0]
        .get_child("tie")
        .is_none());
    let body = n("C")
        .duration(4)
        .kind("whole")
        .extra("<notations id=\"original-notations\"/>")
        .xml()
        .replace("<note>", "<note id=\"original-note\">")
        + &backup(3)
        + &n("D").voice("2").xml();
    let result =
        parse_score(split_score(&score(&[("S", vec![body])]), "chords", None).unwrap()).unwrap();
    assert_eq!(pitches(&result.parts[2], 0), [["C"], ["D"], ["C"]]);
    assert_eq!(
        result.parts[2].measures[0]
            .events
            .iter()
            .map(|e| (e.start.clone(), e.duration.clone()))
            .collect::<Vec<_>>(),
        [(frac(0), frac(1)), (frac(1), frac(1)), (frac(2), frac(2))]
    );
    for part in &result.parts[1..] {
        let ids = ids(&part.source);
        assert_eq!(ids.len(), ids.iter().collect::<HashSet<_>>().len());
    }
}

#[test]
fn selective_split_and_id_collisions_and_invalid_requests() {
    let chord = || n("C").xml() + &n("E").chord().xml();
    let mut original = score(&[("Same", vec![chord()]), ("Same", vec![chord()])]);
    original.parts[0]
        .definition
        .get_mut_child("part-name")
        .unwrap()
        .attributes
        .insert("id".into(), "P1_split_1".into());
    // Reserve the collision in the document as well as the parsed definition.
    original
        .root
        .get_mut_child("part-list")
        .unwrap()
        .get_mut_child("score-part")
        .unwrap()
        .get_mut_child("part-name")
        .unwrap()
        .attributes
        .insert("id".into(), "P1_split_1".into());
    let selected = vec!["P1".to_owned()];
    let result = parse_score(split_score(&original, "chords", Some(&selected)).unwrap()).unwrap();
    assert_eq!(names(&result), ["Same", "Same 1", "Same 2", "Same"]);
    assert_eq!(result.parts[1].id(), "P1_split_1_");
    assert_eq!(result.parts[3].source, original.parts[1].source);
    assert_eq!(
        parse_score(split_score(&original, "chords", None).unwrap())
            .unwrap()
            .parts
            .len(),
        6
    );
    for selected in [
        vec![],
        vec!["missing".into()],
        vec!["P1".into(), "P1".into()],
    ] {
        assert!(split_score(&original, "chords", Some(&selected)).is_err());
    }
    assert!(split_score(&original, "invalid", None)
        .unwrap_err()
        .to_string()
        .contains("mode"));
}
