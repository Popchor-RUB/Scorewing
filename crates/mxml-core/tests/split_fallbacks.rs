mod common;
use common::*;
use mxml_core::{
    frac, parse_score, read_score_bytes, split_plan, split_score, split_score_with_fallbacks,
    split_score_with_options, validate_score, Score, SplitFallback, SplitOptions,
};

fn settings(score: &Score, mode: &str) -> Vec<SplitFallback> {
    score
        .parts
        .iter()
        .flat_map(|part| {
            split_plan(part, mode)
                .unwrap()
                .into_iter()
                .map(|lane| SplitFallback {
                    part_id: part.id().to_owned(),
                    voice: lane.voice,
                    tone: lane.tone,
                    fallback_voice: lane.fallback_voice,
                })
        })
        .collect()
}

fn split(score: &Score, mode: &str, choices: &[SplitFallback]) -> Score {
    let root = split_score_with_fallbacks(score, mode, None, choices)
        .unwrap()
        .root;
    validate_score(&root).unwrap();
    parse_score(root).unwrap()
}

fn stem(step: &str, voice: &str, direction: &str) -> String {
    n(step)
        .voice(voice)
        .extra(&format!("<stem>{direction}</stem>"))
        .xml()
}

fn tied(note: Note, kind: &str) -> String {
    note.xml()
        .replacen("<voice>", &format!("<tie type=\"{kind}\"/><voice>"), 1)
}

fn choir() -> Score {
    score(&[(
        "Choir",
        vec![
            stem("C", "1", "up")
                + &backup(1)
                + &stem("E", "2", "down")
                + &n("G").voice("2").chord().extra("<stem>down</stem>").xml(),
            stem("D", "1", "up")
                + &backup(1)
                + &stem("F", "2", "down")
                + &backup(1)
                + &stem("A", "3", "down"),
            stem("E", "1", "up"),
        ],
    )])
}

#[test]
fn preview_and_configured_export_follow_stem_suggestions_without_lone_voice_sharing() {
    let original = choir();
    let plan = split_plan(&original.parts[0], "voices").unwrap();
    assert_eq!(plan.len(), 3);
    assert_eq!(
        plan.iter()
            .map(|lane| lane.voice.as_str())
            .collect::<Vec<_>>(),
        ["1", "2", "3"]
    );
    assert_eq!(plan[2].fallback_voice.as_deref(), Some("2"));
    assert_eq!(plan[0].fallback_voice, None);
    let chords = split_plan(&original.parts[0], "chords").unwrap();
    assert_eq!(chords.len(), 3);
    assert_eq!(
        chords
            .iter()
            .map(|lane| (lane.voice.as_str(), lane.tone))
            .collect::<Vec<_>>(),
        [("1", 0), ("2", 0), ("2", 1)]
    );
    let result = split(&original, "voices", &settings(&original, "voices"));
    assert_eq!(result.parts[0].source, original.parts[0].source);
    assert_eq!(pitches(&result.parts[3], 0), [["E", "G"]]);
    assert_eq!(pitches(&result.parts[3], 1), [["A"]]);
    assert_eq!(pitches(&result.parts[3], 2), [["rest"]]);
    assert_eq!(pitches(&result.parts[2], 2), [["rest"]]);
    let legacy = parse_score(split_score(&original, "voices", None).unwrap().root).unwrap();
    assert_eq!(pitches(&legacy.parts[3], 0), [["C"]]);
    assert_eq!(pitches(&legacy.parts[3], 2), [["E"]]);
}

#[test]
fn overrides_none_and_chord_tone_selection_are_respected() {
    let original = choir();
    let mut choices = settings(&original, "voices");
    choices[2].fallback_voice = Some("1".into());
    assert_eq!(
        pitches(&split(&original, "voices", &choices).parts[3], 0),
        [["C"]]
    );
    choices[2].fallback_voice = None;
    let silent = split(&original, "voices", &choices);
    assert_eq!(pitches(&silent.parts[3], 0), [["rest"]]);
    assert_eq!(silent.parts[3].measures[0].events[0].duration, frac(1));
    assert_eq!(
        silent.parts[3].measures[0].length,
        original.parts[0].measures[0].length
    );
    let choices = settings(&original, "chords");
    let result = split(&original, "chords", &choices);
    assert_eq!(result.parts.len(), 4);
    assert_eq!(pitches(&result.parts[3], 1), [["A"]]);
    assert_eq!(pitches(&result.parts[2], 0), [["G"]]);
    assert_eq!(pitches(&result.parts[3], 0), [["E"]]);
}

#[test]
fn written_rests_and_partial_measure_gaps_prevent_fallbacks() {
    let original = score(&[(
        "Choir",
        vec![
            n("C").duration(2).xml() + &backup(2) + &n("E").rest().voice("2").xml(),
            n("D").duration(2).xml() + &backup(2) + &forward(1) + &n("F").voice("2").xml(),
        ],
    )]);
    let mut choices = settings(&original, "voices");
    choices[1].fallback_voice = Some("1".into());
    let result = split(&original, "voices", &choices);
    assert_eq!(pitches(&result.parts[2], 0), [["rest"], ["rest"]]);
    assert_eq!(pitches(&result.parts[2], 1), [["rest"], ["F"]]);
}

#[test]
fn grace_notes_and_voice_directions_follow_the_measure_source() {
    let direction = |voice: &str| {
        format!("<direction><direction-type><words>Voice {voice}</words></direction-type><voice>{voice}</voice></direction>")
    };
    let original = score(&[(
        "Choir",
        vec![
            direction("1")
                + &n("B").grace().xml()
                + &n("C").duration(2).xml()
                + &backup(2)
                + &direction("2")
                + &n("E").grace().voice("2").xml()
                + &n("F").voice("2").xml(),
            direction("1") + &n("A").grace().xml() + &n("G").xml(),
        ],
    )]);
    let mut choices = settings(&original, "voices");
    choices[1].fallback_voice = Some("1".into());
    let result = split(&original, "voices", &choices);
    let lower = &result.parts[2];
    assert_eq!(
        text(&lower.measures[0].events[0].notes[0], "pitch/step"),
        "E"
    );
    assert_eq!(
        lower.measures[0]
            .events
            .iter()
            .filter(|event| event.grace())
            .count(),
        1
    );
    assert_eq!(
        text(
            &lower.measures[0]
                .extras
                .iter()
                .find(|extra| extra.element.name == "direction")
                .unwrap()
                .element,
            "direction-type/words"
        ),
        "Voice 2"
    );
    assert_eq!(
        text(&lower.measures[1].events[0].notes[0], "pitch/step"),
        "A"
    );
    assert_eq!(
        text(
            &lower.measures[1]
                .extras
                .iter()
                .find(|extra| extra.element.name == "direction")
                .unwrap()
                .element,
            "direction-type/words"
        ),
        "Voice 1"
    );
    choices[1].fallback_voice = None;
    let result = split(&original, "voices", &choices);
    assert!(result.parts[2].measures[1]
        .events
        .iter()
        .all(|event| !event.grace()));
    assert!(result.parts[2].measures[1]
        .extras
        .iter()
        .all(|extra| extra.element.name != "direction"));
}

#[test]
fn grace_only_presence_prevents_a_fallback_for_the_rest_of_the_measure() {
    let original = score(&[(
        "Choir",
        vec![n("C").xml() + &backup(1) + &n("E").voice("2").grace().xml()],
    )]);
    let mut choices = settings(&original, "voices");
    choices[1].fallback_voice = Some("1".into());
    let result = split(&original, "voices", &choices);
    assert_eq!(pitches(&result.parts[2], 0), [["rest"]]);
    assert_eq!(
        text(
            &result.parts[2].measures[0].events[0].notes[0],
            "pitch/step"
        ),
        "E"
    );
}

#[test]
fn suggestions_use_coverage_and_reject_ties_unknown_and_conflicting_stems() {
    let build = |candidate_extra: &str| {
        score(&[(
            "S",
            vec![
                stem("C", "target", "down"),
                stem("D", "a", "down") + &backup(1) + &stem("E", "b", "down"),
                stem("F", "a", "down") + candidate_extra,
            ],
        )])
    };
    let original = build("");
    let target = |original: &Score| {
        split_plan(&original.parts[0], "voices")
            .unwrap()
            .into_iter()
            .find(|lane| lane.voice == "target")
            .unwrap()
            .fallback_voice
    };
    assert_eq!(target(&original).as_deref(), Some("a"));
    assert_eq!(target(&build(&(backup(1) + &stem("G", "b", "down")))), None);
    let conflicting = score(&[(
        "S",
        vec![
            stem("C", "target", "up") + &stem("D", "target", "down"),
            stem("E", "a", "down"),
        ],
    )]);
    assert_eq!(target(&conflicting), None);
    let unknown = score(&[(
        "S",
        vec![n("C").voice("target").xml(), stem("E", "a", "down")],
    )]);
    assert_eq!(target(&unknown), None);
    assert!(
        split_plan(&score(&[("S", vec![n("C").xml()])]).parts[0], "voices")
            .unwrap()
            .is_empty()
    );
    assert!(split_plan(&original.parts[0], "invalid").is_err());
}

#[test]
fn rejects_incomplete_duplicate_unknown_unselected_and_self_referencing_settings() {
    let original = choir();
    let choices = settings(&original, "voices");
    let fails = |choices: &[SplitFallback]| {
        assert!(split_score_with_fallbacks(&original, "voices", None, choices).is_err())
    };
    fails(&choices[..2]);
    let mut invalid = choices.clone();
    invalid.push(choices[0].clone());
    fails(&invalid);
    let mut invalid = choices.clone();
    invalid[0].voice = "unknown".into();
    fails(&invalid);
    let mut invalid = choices.clone();
    invalid[0].tone = 1;
    fails(&invalid);
    let mut invalid = choices.clone();
    invalid[0].part_id = "unknown".into();
    fails(&invalid);
    let mut invalid = choices.clone();
    invalid[0].fallback_voice = Some("unknown".into());
    fails(&invalid);
    let mut invalid = choices.clone();
    invalid[0].fallback_voice = Some("1".into());
    fails(&invalid);
    let multi = score(&[
        (
            "A",
            vec![n("C").xml() + &backup(1) + &n("E").voice("2").xml()],
        ),
        (
            "B",
            vec![n("D").xml() + &backup(1) + &n("F").voice("2").xml()],
        ),
    ]);
    assert!(split_score_with_fallbacks(
        &multi,
        "voices",
        Some(&["P1".into()]),
        &settings(&multi, "voices")
    )
    .is_err());
    assert!(split_score_with_fallbacks(&original, "voices", Some(&[]), &choices).is_err());
}

#[test]
fn fallbacks_do_not_chain_when_the_selected_source_is_absent() {
    let original = choir();
    let mut choices = settings(&original, "voices");
    choices[1].fallback_voice = Some("1".into());
    choices[2].fallback_voice = Some("2".into());
    let result = split(&original, "voices", &choices);
    assert_eq!(pitches(&result.parts[2], 2), [["E"]]);
    assert_eq!(pitches(&result.parts[3], 2), [["rest"]]);
}

#[test]
fn fallback_ties_cross_matching_pitches_and_mismatched_pitches_become_slurs() {
    for (step, expected) in [("C", "tie"), ("D", "slur")] {
        let original = score(&[(
            "S",
            vec![
                tied(
                    n("C").extra("<notations><tied type=\"start\"/></notations>"),
                    "start",
                ),
                n(step).voice("2").xml(),
            ],
        )]);
        let mut choices = settings(&original, "voices");
        choices[1].fallback_voice = Some("1".into());
        let transformed = split_score_with_fallbacks(&original, "voices", None, &choices).unwrap();
        assert_eq!(transformed.warnings.len(), usize::from(expected == "slur"));
        validate_score(&transformed.root).unwrap();
        let result = parse_score(transformed.root).unwrap();
        let lower = &result.parts[2];
        for (measure, kind) in [(0, "start"), (1, "stop")] {
            let note = &lower.measures[measure].events[0].notes[0];
            assert_eq!(
                note.attributes.get("color").map(String::as_str),
                (expected == "slur").then_some("#FF0000")
            );
            assert!(descendants(note).iter().any(|node| node.name == expected
                && node
                    .attributes
                    .get("type")
                    .is_some_and(|value| value == kind)));
        }
    }
}

#[test]
fn supplied_scores_mark_only_inserted_slurs_and_report_the_source_locations() {
    for (name, source, expected) in [
        (
            "Test",
            include_bytes!("fixtures/Test.mxl").as_slice(),
            include_bytes!("fixtures/Test-arranged-expected.mxl").as_slice(),
        ),
        (
            "Test2",
            include_bytes!("fixtures/Test2.mxl").as_slice(),
            include_bytes!("fixtures/Test2-arranged-expected.mxl").as_slice(),
        ),
    ] {
        let original = read_score_bytes(source, name).unwrap();
        let expected = read_score_bytes(expected, name).unwrap();
        for mode in ["voices", "chords"] {
            let mut choices = settings(&original, mode);
            for choice in &mut choices {
                choice.fallback_voice = (choice.voice != "1").then(|| "1".into());
            }
            let transformed = split_score_with_fallbacks(&original, mode, None, &choices).unwrap();
            assert_eq!(transformed.warnings.len(), 1, "{name}, {mode}");
            let warning = &transformed.warnings[0];
            let source_voice = if name == "Test" { "2" } else { "3" };
            for location in [
                "measure 1",
                "measure 2",
                "voice 1",
                "notes [E4]",
                "notes [A4]",
                &format!("voice {source_voice}"),
                "inserted slur",
            ] {
                assert!(warning.contains(location), "{warning}: missing {location}");
            }
            validate_score(&transformed.root).unwrap();
            let output = parse_score(transformed.root).unwrap();
            assert_eq!(output.parts.len(), expected.parts.len());
            for (part_index, (actual, expected)) in
                output.parts.iter().zip(&expected.parts).enumerate()
            {
                assert_eq!(actual.measures.len(), expected.measures.len());
                for (measure_index, (actual, expected)) in
                    actual.measures.iter().zip(&expected.measures).enumerate()
                {
                    assert_eq!(actual.events.len(), expected.events.len());
                    for (event_index, (actual, expected)) in
                        actual.events.iter().zip(&expected.events).enumerate()
                    {
                        assert_eq!(
                            (&actual.start, &actual.duration, &actual.voice),
                            (&expected.start, &expected.duration, &expected.voice)
                        );
                        assert_eq!(actual.notes.len(), expected.notes.len());
                        for (actual, expected) in actual.notes.iter().zip(&expected.notes) {
                            for tag in ["pitch", "rest", "type", "dot"] {
                                assert_eq!(
                                    actual.get_child(tag),
                                    expected.get_child(tag),
                                    "{name}: {tag}"
                                );
                            }
                            let relations = |note: &xmltree::Element| {
                                descendants(note)
                                    .into_iter()
                                    .filter(|node| {
                                        ["tie", "tied", "slur"].contains(&node.name.as_str())
                                    })
                                    .map(|node| {
                                        (node.name.clone(), node.attributes.get("type").cloned())
                                    })
                                    .collect::<Vec<_>>()
                            };
                            assert_eq!(relations(actual), relations(expected), "{name}, {mode}");
                            // The expected files establish the music; the request adds red endpoints.
                            let red = part_index == output.parts.len() - 1 && event_index == 0;
                            assert_eq!(
                                actual.attributes.get("color").map(String::as_str),
                                red.then_some("#FF0000"),
                                "{name}, {mode}, part {part_index}, measure {measure_index}"
                            );
                            for slur in descendants(actual)
                                .into_iter()
                                .filter(|node| node.name == "slur")
                            {
                                assert_eq!(
                                    slur.attributes.get("color").map(String::as_str),
                                    Some("#FF0000")
                                );
                            }
                        }
                    }
                }
            }
            assert_eq!(output.parts[0].source, original.parts[0].source);
        }
    }
}

#[test]
fn disabling_fallback_slurs_rests_the_fallback_note_and_preserves_ties() {
    for source in [
        include_bytes!("fixtures/Test.mxl").as_slice(),
        include_bytes!("fixtures/Test2.mxl").as_slice(),
    ] {
        let original = read_score_bytes(source, "fallback fixture").unwrap();
        for mode in ["voices", "chords"] {
            let mut choices = settings(&original, mode);
            for choice in &mut choices {
                choice.fallback_voice = (choice.voice != "1").then(|| "1".into());
            }
            let enabled = split_score_with_fallbacks(&original, mode, None, &choices).unwrap();
            let disabled = split_score_with_options(
                &original,
                mode,
                None,
                SplitOptions {
                    fallbacks: Some(&choices),
                    insert_fallback_slurs: false,
                },
            )
            .unwrap();
            validate_score(&disabled.root).unwrap();
            assert!(disabled.warnings.is_empty());
            assert!(descendants(&disabled.root).iter().all(|node| {
                node.name != "slur" && node.attributes.get("color").is_none_or(|c| c != "#FF0000")
            }));
            let enabled = parse_score(enabled.root).unwrap();
            let disabled = parse_score(disabled.root).unwrap();
            assert_eq!(disabled.parts[0].source, original.parts[0].source);
            assert_eq!(disabled.parts.len(), enabled.parts.len());
            for (part_index, (actual, expected)) in
                disabled.parts.iter().zip(&enabled.parts).enumerate()
            {
                assert_eq!(actual.measures.len(), expected.measures.len());
                for (measure_index, (actual, expected)) in
                    actual.measures.iter().zip(&expected.measures).enumerate()
                {
                    assert_eq!(actual.events.len(), expected.events.len());
                    for (event_index, (actual, expected)) in
                        actual.events.iter().zip(&expected.events).enumerate()
                    {
                        assert_eq!(
                            (&actual.start, &actual.duration),
                            (&expected.start, &expected.duration)
                        );
                        assert_eq!(actual.notes.len(), expected.notes.len());
                        for (actual, expected) in actual.notes.iter().zip(&expected.notes) {
                            if part_index == disabled.parts.len() - 1
                                && measure_index == 1
                                && event_index == 0
                            {
                                assert!(actual.get_child("rest").is_some());
                                assert!(actual.get_child("pitch").is_none());
                                assert_eq!(
                                    mxml_core::xml::child_text(actual, "type"),
                                    mxml_core::xml::child_text(expected, "type")
                                );
                            } else {
                                for tag in ["pitch", "rest", "tie"] {
                                    assert_eq!(
                                        actual.get_child(tag),
                                        expected.get_child(tag),
                                        "{mode}: {tag}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn disabled_slurs_rest_only_fallback_endpoints_in_either_direction() {
    for native_first in [true, false] {
        for step in ["C", "D"] {
            for (ticks, divisions) in [(3, 2), (2, 3)] {
                let fallback = n("C").duration(ticks);
                let native = n(step).voice("2");
                let (left, right) = if native_first {
                    (native, fallback)
                } else {
                    (fallback, native)
                };
                let original =
                    score_div(&[("S", vec![tied(left, "start"), right.xml()])], divisions);
                let mut choices = settings(&original, "voices");
                choices[1].fallback_voice = Some("1".into());
                let result = split_score_with_options(
                    &original,
                    "voices",
                    None,
                    SplitOptions {
                        fallbacks: Some(&choices),
                        insert_fallback_slurs: false,
                    },
                )
                .unwrap();
                assert!(result.warnings.is_empty());
                validate_score(&result.root).unwrap();
                let output = parse_score(result.root).unwrap();
                let lane = &output.parts[2];
                let fallback_index = usize::from(native_first);
                let fallback = &lane.measures[fallback_index].events[0];
                let native = &lane.measures[1 - fallback_index].events[0];
                assert_eq!(
                    fallback.duration,
                    frac(ticks.into()) / frac(divisions.into())
                );
                assert_eq!(native.duration, frac(1) / frac(divisions.into()));
                assert!(native.notes[0].get_child("pitch").is_some());
                assert_eq!(fallback.notes[0].get_child("rest").is_some(), step != "C");
                for (measure, kind) in [(0, "start"), (1, "stop")] {
                    let note = &lane.measures[measure].events[0].notes[0];
                    assert!(descendants(note).iter().all(|e| e.name != "slur"));
                    assert_eq!(
                        note.get_child("tie")
                            .map(|tie| tie.attributes["type"].as_str()),
                        (step == "C").then_some(kind)
                    );
                }
                assert_eq!(output.parts[0].source, original.parts[0].source);
            }
        }
    }
}

#[test]
fn disabling_fallback_slurs_preserves_existing_slurs() {
    let original = score(&[(
        "S",
        vec![
            n("C")
                .extra("<notations><slur type=\"start\" number=\"1\"/></notations>")
                .xml()
                + &backup(1)
                + &n("E").voice("2").xml(),
            n("D")
                .extra("<notations><slur type=\"stop\" number=\"1\"/></notations>")
                .xml(),
        ],
    )]);
    let mut choices = settings(&original, "voices");
    choices[1].fallback_voice = Some("1".into());
    let enabled = split_score_with_fallbacks(&original, "voices", None, &choices).unwrap();
    let disabled = split_score_with_options(
        &original,
        "voices",
        None,
        SplitOptions {
            fallbacks: Some(&choices),
            insert_fallback_slurs: false,
        },
    )
    .unwrap();
    assert!(disabled.warnings.is_empty());
    assert_eq!(disabled.root, enabled.root);
    let output = parse_score(disabled.root).unwrap();
    assert_eq!(
        descendants(&output.parts[1].source)
            .iter()
            .filter(|node| node.name == "slur")
            .count(),
        2
    );
}

#[test]
fn silent_boundaries_remove_orphan_ties_and_slurs() {
    let original = score(&[("S", vec![
        tied(n("C").voice("2").extra("<notations><tied type=\"start\"/><slur type=\"start\" number=\"1\"/></notations>"), "start"),
        n("D").xml(),
        tied(n("C").voice("2").extra("<notations><tied type=\"stop\"/><slur type=\"stop\" number=\"1\"/></notations>"), "stop"),
    ])]);
    let choices = settings(&original, "voices");
    let result = split(&original, "voices", &choices);
    let lower = &result.parts[2];
    assert!(descendants(&lower.source)
        .iter()
        .all(|node| node.name != "tie" && node.name != "tied"));
    // Slurs may legitimately span rests when both original endpoints are retained.
    assert_eq!(
        descendants(&lower.source)
            .iter()
            .filter(|node| node.name == "slur")
            .count(),
        2
    );
    let source = score(&[(
        "S",
        vec![
            n("C")
                .extra("<notations><slur type=\"start\" number=\"1\"/></notations>")
                .xml(),
            n("D")
                .extra("<notations><slur type=\"stop\" number=\"1\"/></notations>")
                .xml()
                + &backup(1)
                + &n("E").voice("2").xml(),
        ],
    )]);
    let mut choices = settings(&source, "voices");
    choices[1].fallback_voice = Some("1".into());
    let result = split(&source, "voices", &choices);
    assert!(descendants(&result.parts[2].source)
        .iter()
        .all(|node| node.name != "slur"));
}

#[test]
fn native_chord_relationship_ambiguities_warn_with_explicit_fallbacks() {
    let original = score(&[(
        "S",
        vec![
            tied(
                n("C").extra("<notations><slur type=\"start\" number=\"1\"/></notations>"),
                "start",
            ) + &n("G").octave(3).chord().xml()
                + &tied(
                    n("C").extra("<notations><slur type=\"stop\" number=\"1\"/></notations>"),
                    "stop",
                )
                + &n("B").octave(5).chord().xml(),
        ],
    )]);
    let error =
        split_score_with_fallbacks(&original, "chords", None, &settings(&original, "chords"))
            .unwrap()
            .warnings
            .join("\n");
    assert!(error.contains("ambiguous tie"), "{error}");
    assert!(error.contains("ambiguous slur"), "{error}");
}
