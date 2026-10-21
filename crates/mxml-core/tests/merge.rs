mod common;
use common::*;
use mxml_core::{
    merge::merge_score,
    model::{frac, parse_score, Score},
};
use std::collections::HashSet;

fn merged(parts: &[(&str, Vec<String>)]) -> Score {
    let output = merge_score(&score(parts), &groups(&[&["P1", "P2"]])).unwrap();
    mxml_core::validate_score(&output).unwrap();
    parse_score(output).unwrap()
}

#[test]
fn ordered_groups_anchor_at_first_score_position_and_keep_unselected_parts() {
    let input = score(&[
        ("Soprano", vec![n("C").xml()]),
        ("Alto", vec![n("E").xml()]),
        ("Tenor", vec![n("G").xml()]),
        ("Bass", vec![n("C").octave(3).xml()]),
    ]);
    let result =
        parse_score(merge_score(&input, &groups(&[&["P2", "P1"], &["P3", "P4"]])).unwrap())
            .unwrap();
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
    let result = parse_score(merge_score(&input, &groups(&[&["P3", "P1"]])).unwrap()).unwrap();
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
fn common_prefix_diverges_and_resets_each_bar() {
    let first = n("C").xml() + &n("D").xml() + &n("E").xml() + &n("F").xml();
    let second = n("C").xml() + &n("E").xml() + &n("E").xml() + &n("F").xml();
    let result = merged(&[
        ("S", vec![first.clone(), first.clone()]),
        ("A", vec![second, first]),
    ]);
    let measures = &result.parts[0].measures;
    assert_eq!(
        measures[0]
            .events
            .iter()
            .map(|e| (e.start.clone(), e.voice.as_str()))
            .collect::<Vec<_>>(),
        [
            (frac(0), "1"),
            (frac(1), "1"),
            (frac(2), "1"),
            (frac(3), "1"),
            (frac(1), "2"),
            (frac(2), "2"),
            (frac(3), "2")
        ]
    );
    assert!(measures[1].events.iter().all(|e| e.voice == "1"));
}

#[test]
fn three_sources_stems_and_common_rests() {
    let input = score(&[
        ("S", vec![n("C").rest().xml() + &n("C").xml()]),
        ("A", vec![n("C").rest().xml() + &n("D").xml()]),
        ("T", vec![n("C").rest().xml() + &n("E").xml()]),
    ]);
    let result =
        parse_score(merge_score(&input, &groups(&[&["P1", "P2", "P3"]])).unwrap()).unwrap();
    let events = &result.parts[0].measures[0].events;
    assert_eq!(events.len(), 4);
    assert!(events[0].notes[0].get_child("rest").is_some());
    assert_eq!(
        events[1..]
            .iter()
            .map(|e| text(&e.notes[0], "stem"))
            .collect::<Vec<_>>(),
        ["up", "down", "up"]
    );
}

#[test]
fn flatten_chords_deduplicate_unisons_and_reject_overlapping_rhythmic_spans() {
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
    assert_eq!(result.parts[0].measures[0].events.len(), 1);
    assert_eq!(result.parts[0].measures[0].events[0].notes.len(), 1);
    let incompatible =
        n("C").duration(2).xml() + &backup(2) + &n("E").voice("2").xml() + &n("F").voice("2").xml();
    let input = score(&[
        ("S", vec![incompatible]),
        ("A", vec![n("C").duration(2).xml()]),
    ]);
    assert!(merge_score(&input, &groups(&[&["P1", "P2"]]))
        .unwrap_err()
        .to_string()
        .contains("measure 1, position 0: incompatible overlapping"));
}

#[test]
fn rhythms_spelled_pitches_and_sound_or_visual_tie_differences_do_not_collapse() {
    let result = merged(&[
        ("S", vec![n("C").duration(2).kind("half").xml()]),
        ("A", vec![n("C").xml() + &n("D").xml()]),
    ]);
    assert!(result.parts[0].measures[0]
        .events
        .iter()
        .any(|e| e.voice == "2"));
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
        assert_eq!(
            merged(&[("S", vec![tied]), ("A", vec![n("C").xml()])]).parts[0].measures[0]
                .events
                .len(),
            2
        );
    }
}

#[test]
fn differing_graces_cut_sustained_note_with_ties_and_unique_identifiers() {
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
        .filter(|e| !e.grace())
        .collect();
    assert_eq!(
        ordinary
            .iter()
            .map(|e| (e.start.clone(), e.duration.clone(), e.voice.as_str()))
            .collect::<Vec<_>>(),
        [
            (frac(0), frac(2), "1"),
            (frac(2), frac(2), "1"),
            (frac(2), frac(2), "2")
        ]
    );
    assert_eq!(
        ordinary
            .iter()
            .map(|e| e.notes[0].get_child("tie").unwrap().attributes["type"].as_str())
            .collect::<Vec<_>>(),
        ["start", "stop", "stop"]
    );
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
        [("D".into(), true), ("E".into(), true), ("C".into(), false)]
    );
}

#[test]
fn lyric_lines_stable_and_identical_text_deduplicated() {
    let lyric = |text: &str| format!("<lyric number=\"1\"><text>{text}</text></lyric>");
    let upper = n("C").extra(&lyric("hello")).xml() + &n("D").extra(&lyric("same")).xml();
    let lower = n("C").extra(&lyric("world")).xml() + &n("D").extra(&lyric("same")).xml();
    let result = merged(&[("S", vec![upper]), ("A", vec![lower])]);
    let events = &result.parts[0].measures[0].events;
    assert_eq!(
        elements(&events[0].notes[0])
            .filter(|n| n.name == "lyric")
            .map(|n| (n.attributes["number"].as_str(), text(n, "text")))
            .collect::<Vec<_>>(),
        [("1", "hello".into()), ("2", "world".into())]
    );
    assert_eq!(
        elements(&events[1].notes[0])
            .filter(|n| n.name == "lyric")
            .count(),
        1
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
    let note = &measure.events[0].notes[0];
    assert!(note.get_child("stem").is_none());
    assert_eq!(
        elements(
            note.get_child("notations")
                .unwrap()
                .get_child("articulations")
                .unwrap()
        )
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
    mxml_core::xml::set_child(
        second
            .get_mut_child("measure")
            .unwrap()
            .get_mut_child("attributes")
            .unwrap(),
        "divisions",
        "3",
    );
    input = parse_score(input.root).unwrap();
    let result = parse_score(merge_score(&input, &groups(&[&["P1", "P2"]])).unwrap()).unwrap();
    assert_eq!(
        result.parts[0].measures[0]
            .events
            .iter()
            .map(|e| (e.start.clone(), e.duration.clone()))
            .collect::<Vec<_>>(),
        [(frac(0), frac(1)), (frac(1), frac(1) / frac(3))]
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
    assert_eq!(
        parse_score(merge_score(&input, &groups(&[&["P1", "P2"]])).unwrap())
            .unwrap()
            .parts[0]
            .measures[0]
            .events
            .len(),
        2
    );
}

#[test]
fn relationships_renumbered_before_prefix_merge() {
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
        [("start", "1"), ("start", "2"), ("stop", "1"), ("stop", "2")]
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
    assert_eq!(text(note, "lyric/text"), "word");
    assert!(note.get_child("play").is_some());
    let mut input = score(&[
        (
            "S",
            vec![n("C").extra("<play><mute>off</mute></play>").xml() + &n("E").xml()],
        ),
        (
            "A",
            vec![
                n("D")
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
            element("<part-name-display><display-text>OLD NAME</display-text></part-name-display>"),
        );
        append(&mut part.definition,element(&format!("<score-instrument id=\"I{index}\"><instrument-name>Choir</instrument-name></score-instrument>")));
        append(
            &mut part.definition,
            element(&format!(
                "<player id=\"Player{index}\"><player-name>Singer</player-name></player>"
            )),
        );
        append(&mut part.definition,element(&format!("<midi-instrument id=\"I{index}\"><midi-channel>{index}</midi-channel></midi-instrument>")));
    }
    append(&mut input.parts[1].definition,element("<score-instrument id=\"AltOverride\"><instrument-name>Alternate alto</instrument-name></score-instrument>"));
    mxml_core::xml::insert_note_child(
        &mut input.parts[1].measures[0].events[1].notes[0],
        element("<instrument id=\"AltOverride\"/>"),
    );
    let result = parse_score(merge_score(&input, &groups(&[&["P1", "P2"]])).unwrap()).unwrap();
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
        [("1", "I1"), ("1", "I1"), ("2", "I2"), ("2", "AltOverride")]
    );
    assert!(events[0].notes[0].get_child("play").is_some());
    assert_eq!(text(&events[2].notes[0], "lyric/text"), "word");
}
