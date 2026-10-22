mod common;
use common::*;
use mxml_core::{
    frac, parse_score, split_plan, split_score, split_score_with_fallbacks, validate_score, Score,
    SplitFallback,
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
    let root = split_score_with_fallbacks(score, mode, None, choices).unwrap();
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
    let legacy = parse_score(split_score(&original, "voices", None).unwrap()).unwrap();
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
        let result = split(&original, "voices", &choices);
        let lower = &result.parts[2];
        for (measure, kind) in [(0, "start"), (1, "stop")] {
            assert!(descendants(&lower.measures[measure].events[0].notes[0])
                .iter()
                .any(|node| node.name == expected
                    && node
                        .attributes
                        .get("type")
                        .is_some_and(|value| value == kind)));
        }
    }
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
fn native_chord_relationship_ambiguities_still_fail_with_explicit_fallbacks() {
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
            .unwrap_err()
            .to_string();
    assert!(error.contains("ambiguous tie"), "{error}");
    assert!(error.contains("ambiguous slur"), "{error}");
}
