mod common;

use common::*;
use scorewing_core::{
    merge::merge_score,
    model::{frac, parse_fraction, parse_score, Part, Score},
    split::{split_plan, split_score},
    xml::{child_text, children},
};

fn imported_score() -> Score {
    let marks = "<direction><direction-type><words>Allegro</words></direction-type><direction-type><metronome><beat-unit>quarter</beat-unit><per-minute>120</per-minute></metronome></direction-type><offset>-1</offset><voice>2</voice><sound tempo=\"120\"/></direction><direction><direction-type><rehearsal>B</rehearsal></direction-type><offset>-1</offset><voice>2</voice></direction>";
    let chord = n("C").xml() + &n("E").chord().xml();
    let mut score = score(&[
        ("Visible", vec![chord.clone() + marks, chord.clone()]),
        ("Hidden", vec![chord.clone(), chord.clone()]),
        ("Piano", vec![chord.clone() + marks, chord]),
    ]);
    let parts: Vec<_> = score
        .root
        .children
        .iter_mut()
        .filter_map(xmltree::XMLNode::as_mut_element)
        .filter(|node| node.name == "part")
        .collect();
    for (index, part) in parts.into_iter().enumerate() {
        let first = part.get_mut_child("measure").unwrap();
        if index == 1 {
            append(
                first.get_mut_child("attributes").unwrap(),
                element("<staff-details print-object=\"no\"/>"),
            );
        }
        if index == 2 {
            let attributes = first.get_mut_child("attributes").unwrap();
            let before_clef = attributes
                .children
                .iter()
                .position(|node| node.as_element().is_some_and(|node| node.name == "clef"))
                .unwrap();
            attributes.children.insert(
                before_clef,
                xmltree::XMLNode::Element(element("<staves>2</staves>")),
            );
            append(
                first.get_mut_child("attributes").unwrap(),
                element("<staff-details number=\"1\" print-object=\"no\"/>"),
            );
            // Same-voice overlaps are valid across original staves, but unsupported by the single-staff parser.
            append(first, element("<backup><duration>1</duration></backup>"));
            append(first, element("<note><pitch><step>G</step><octave>3</octave></pitch><duration>1</duration><staff>2</staff></note>"));
            append(first, element("<direction id=\"scorewing-tempo-native\"><direction-type><words>rit.</words></direction-type><offset>-1</offset></direction>"));
            append(first, element("<direction placement=\"below\"><direction-type><words>Tempo reminder</words></direction-type><offset>-1</offset><sound tempo=\"120.0\"/></direction>"));
            append(first, element("<direction><direction-type><words>local instruction</words></direction-type></direction>"));
            let second = part
                .children
                .iter_mut()
                .filter_map(xmltree::XMLNode::as_mut_element)
                .filter(|node| node.name == "measure")
                .nth(1)
                .unwrap();
            append(second, element("<direction><direction-type><words>Andante</words></direction-type><offset>-0.5</offset><sound tempo=\"80\"/></direction>"));
        }
    }
    parse_score(score.root).unwrap()
}

#[test]
fn split_and_merge_keep_scorewide_marks_only_on_the_first_staff() {
    let original = imported_score();
    for root in [
        split_score(&original, "chords", None).unwrap().root,
        split_score(&original, "voices", None).unwrap().root,
        merge_score(&original, &groups(&[&["P2", "P1"]])).unwrap(),
    ] {
        scorewing_core::validate_score(&root).unwrap();
        let output = parse_score(root).unwrap();
        let carriers: Vec<_> = output
            .parts
            .iter()
            .filter(|part| {
                descendants(&part.source).iter().any(|node| {
                    matches!(node.name.as_str(), "rehearsal" | "metronome")
                        || node.name == "sound" && node.attributes.contains_key("tempo")
                })
            })
            .map(|part| part.id())
            .collect();
        assert_eq!(carriers, [output.parts[0].id()]);
    }
}

#[test]
fn unselected_hidden_marks_keep_fractional_beats_and_source_timing() {
    let mut original = score(&[
        ("First", vec![
            n("C").xml() + "<attributes><divisions>2</divisions></attributes>" + &n("D").duration(2).xml(),
            n("E").duration(2).xml(),
        ]),
        ("Hidden carrier", vec![
            n("F").xml() + "<direction><direction-type><rehearsal>A</rehearsal></direction-type><offset>0.5</offset></direction><sound tempo=\"90\"><offset>1</offset></sound><direction><direction-type><words>Lento</words></direction-type><offset>0.5</offset><sound tempo=\"70\"/></direction>",
            n("G").xml() + "<direction><direction-type><words>Andante</words></direction-type><offset>1</offset><sound tempo=\"80\"><offset>2</offset></sound></direction>",
        ]),
        ("Selected", vec![n("A").xml() + &n("C").chord().xml(), n("B").xml() + &n("D").chord().xml()]),
        ("Other selected", vec![n("E").xml(), n("F").xml()]),
    ]);
    let carrier = original
        .root
        .children
        .iter_mut()
        .filter_map(scorewing_core::XMLNode::as_mut_element)
        .filter(|node| node.name == "part")
        .nth(1)
        .unwrap();
    let attributes = carrier
        .get_mut_child("measure")
        .unwrap()
        .get_mut_child("attributes")
        .unwrap();
    scorewing_core::set_child(attributes, "divisions", "3");
    append(attributes, element("<staff-details print-object=\"no\"/>"));
    let original = parse_score(original.root).unwrap();
    for root in [
        split_score(&original, "chords", Some(&["P3".into()]))
            .unwrap()
            .root,
        merge_score(&original, &groups(&[&["P3", "P4"]])).unwrap(),
    ] {
        scorewing_core::validate_score(&root).unwrap();
        let output = parse_score(root).unwrap();
        let at = |extra: &scorewing_core::TimedElement| {
            &extra.position
                + parse_fraction(&child_text(&extra.element, "offset").unwrap()).unwrap()
                    / &extra.divisions
        };
        let first = &output.parts[0];
        let marks: Vec<_> = first.measures[0]
            .extras
            .iter()
            .filter(|extra| matches!(extra.element.name.as_str(), "direction" | "sound"))
            .collect();
        assert_eq!(marks.len(), 3);
        assert_eq!(at(marks[0]), frac(1) / frac(2));
        assert_eq!(at(marks[1]), frac(2) / frac(3));
        assert_eq!(at(marks[2]), frac(1) / frac(2));
        let playback = marks[2].element.get_child("sound").unwrap();
        assert_eq!(
            parse_fraction(&child_text(playback, "offset").unwrap()).unwrap() / &marks[2].divisions,
            frac(1) / frac(3)
        );
        let later = first.measures[1]
            .extras
            .iter()
            .find(|extra| extra.element.name == "direction")
            .unwrap();
        assert_eq!(at(later), frac(2) / frac(3));
        let playback_offset =
            child_text(later.element.get_child("sound").unwrap(), "offset").unwrap();
        assert_eq!(
            &later.position + parse_fraction(&playback_offset).unwrap() / &later.divisions,
            frac(1)
        );
        for (before, after) in original.parts[0].measures.iter().zip(&first.measures) {
            assert_eq!(before.length, after.length);
            assert_eq!(
                before
                    .events
                    .iter()
                    .map(|event| (&event.start, &event.duration))
                    .collect::<Vec<_>>(),
                after
                    .events
                    .iter()
                    .map(|event| (&event.start, &event.duration))
                    .collect::<Vec<_>>()
            );
        }
        let hidden = output.parts.iter().find(|part| part.id() == "P2").unwrap();
        assert!(!hidden.visible());
        assert!(!descendants(&hidden.source)
            .iter()
            .any(|node| node.name == "direction" || node.name == "sound"));
    }
}

#[test]
fn shorter_first_staff_reports_unplaceable_marks_instead_of_losing_them() {
    let original = score(&[
        ("First", vec![n("C").xml()]),
        ("Carrier", vec![n("D").xml(), n("E").xml() + "<direction><direction-type><rehearsal>C</rehearsal></direction-type></direction>"]),
        ("Companion", vec![n("F").xml(), n("G").xml()]),
    ]);
    assert!(split_score(&original, "chords", None)
        .unwrap_err()
        .to_string()
        .contains("first staff"));
    assert!(merge_score(&original, &groups(&[&["P2", "P3"]]))
        .unwrap_err()
        .to_string()
        .contains("first staff"));
}

#[test]
fn opaque_first_instrument_places_marks_on_its_first_staff() {
    let mut original = imported_score();
    original
        .root
        .children
        .retain(|node| node.as_element().is_none_or(|node| node.name != "part"));
    let list = original.root.get_mut_child("part-list").unwrap();
    list.children.clear();
    for index in [2, 0, 1] {
        append(list, original.parts[index].definition.clone());
    }
    for index in [2, 0, 1] {
        append(&mut original.root, original.parts[index].source.clone());
    }
    let original = parse_score(original.root).unwrap();
    for root in [
        split_score(&original, "chords", Some(&["P1".into()]))
            .unwrap()
            .root,
        merge_score(&original, &groups(&[&["P1", "P2"]])).unwrap(),
    ] {
        scorewing_core::validate_score(&root).unwrap();
        let first = children(&root, "part").next().unwrap();
        assert_eq!(first.attributes["id"], "P3");
        let directions: Vec<_> = children(first, "measure")
            .flat_map(|measure| children(measure, "direction"))
            .filter(|node| {
                child_text(node.get_child("direction-type").unwrap(), "words").as_deref()
                    != Some("local instruction")
            })
            .collect();
        assert!(!directions.is_empty());
        assert!(directions
            .iter()
            .all(|direction| child_text(direction, "staff").as_deref() == Some("1")));
    }
}

fn assert_preserved(original: &Score, output: &Score, generated: &[&Part]) {
    for part in &original.parts {
        let retained = output
            .parts
            .iter()
            .find(|retained| retained.id() == part.id())
            .unwrap();
        for tag in ["note", "clef", "key", "time", "barline", "staff-details"] {
            let nodes = |source: &scorewing_core::Element| {
                descendants(source)
                    .into_iter()
                    .filter(|node| node.name == tag)
                    .cloned()
                    .collect::<Vec<_>>()
            };
            assert_eq!(nodes(&retained.source), nodes(&part.source), "{tag}");
        }
        let events = |part: &Part| {
            part.measures
                .iter()
                .map(|measure| {
                    measure
                        .events
                        .iter()
                        .map(|event| {
                            (
                                event.start.clone(),
                                event.duration.clone(),
                                event.voice.clone(),
                                event.notes.clone(),
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(events(retained), events(part));
        assert_eq!(retained.definition, part.definition);
        assert_eq!(retained.visible(), part.visible());
    }
    assert!(!generated.is_empty());
    for part in generated {
        assert!(part.visible());
        assert!(part.supported());
        assert!(!descendants(&part.source).iter().any(|node| {
            matches!(node.name.as_str(), "metronome" | "rehearsal")
                || node.name == "sound" && node.attributes.contains_key("tempo")
        }));
    }
    let part = &output.parts[0];
    {
        let first = &part.measures[0];
        let directions: Vec<_> = first
            .extras
            .iter()
            .filter(|extra| extra.element.name == "direction")
            .collect();
        let at = |extra: &scorewing_core::model::TimedElement| {
            &extra.position
                + child_text(&extra.element, "offset")
                    .map(|offset| parse_fraction(&offset).unwrap() / &extra.divisions)
                    .unwrap_or_else(|| frac(0))
        };
        for (kind, value) in [("metronome", "120"), ("rehearsal", "B"), ("words", "rit.")] {
            let found: Vec<_> = directions
                .iter()
                .filter(|extra| {
                    children(&extra.element, "direction-type").any(|direction| {
                        children(direction, kind).any(|mark| {
                            if kind == "metronome" {
                                child_text(mark, "per-minute").as_deref() == Some(value)
                            } else {
                                mark.get_text().as_deref() == Some(value)
                            }
                        })
                    })
                })
                .collect();
            assert_eq!(found.len(), 1, "{kind} must occur once on {}", part.id());
            assert_eq!(at(found[0]), frac(0));
        }
        assert_eq!(
            directions
                .iter()
                .filter(|extra| children(&extra.element, "sound").any(|sound| sound
                    .attributes
                    .get("tempo")
                    .is_some_and(|tempo| tempo == "120")))
                .count(),
            1
        );
        assert_eq!(
            directions
                .iter()
                .flat_map(|extra| children(&extra.element, "sound"))
                .filter(|sound| sound.attributes.contains_key("tempo"))
                .count(),
            1
        );
        assert!(directions
            .iter()
            .any(
                |extra| children(&extra.element, "direction-type").any(|kind| child_text(
                    kind, "words"
                )
                .as_deref()
                    == Some("Tempo reminder"))
            ));
        assert!(!directions.iter().any(|extra| child_text(
            extra.element.get_child("direction-type").unwrap(),
            "words"
        )
        .as_deref()
            == Some("local instruction")));
        let later = part.measures[1]
            .extras
            .iter()
            .find(|extra| {
                children(&extra.element, "sound").any(|sound| {
                    sound
                        .attributes
                        .get("tempo")
                        .is_some_and(|tempo| tempo == "80")
                })
            })
            .unwrap();
        assert_eq!(at(later), frac(1) / frac(2));
    }
}

#[test]
fn opaque_multistaff_sources_remain_inspectable_and_cannot_be_selected() {
    let score = imported_score();
    assert!(score.parts[0].visible());
    assert!(!score.parts[1].visible());
    assert_eq!(score.parts[2].staff_count(), 2);
    assert!(score.parts[2].visible());
    assert!(!score.parts[2].supported());
    assert!(score.parts[2].measures.is_empty());
    assert!(split_plan(&score.parts[2], "chords")
        .unwrap_err()
        .to_string()
        .contains("multi-staff"));
    assert!(split_score(&score, "chords", Some(&["P3".into()]))
        .unwrap_err()
        .to_string()
        .contains("multi-staff"));
    assert!(merge_score(&score, &groups(&[&["P1", "P3"]]))
        .unwrap_err()
        .to_string()
        .contains("multi-staff"));
}

#[test]
fn split_preserves_original_music_and_hidden_source_visibility_with_one_mark_carrier() {
    let original = imported_score();
    let result = split_score(&original, "chords", None).unwrap();
    scorewing_core::validate_score(&result.root).unwrap();
    let output = parse_score(result.root).unwrap();
    let generated: Vec<_> = output
        .parts
        .iter()
        .filter(|part| part.id().contains("_split_"))
        .collect();
    assert_eq!(generated.len(), 4);
    assert_preserved(&original, &output, &generated);
}

#[test]
fn merge_preserves_original_music_and_hidden_source_visibility_with_one_mark_carrier() {
    let original = imported_score();
    let result = merge_score(&original, &groups(&[&["P2", "P1"]])).unwrap();
    scorewing_core::validate_score(&result).unwrap();
    let output = parse_score(result).unwrap();
    let generated: Vec<_> = output
        .parts
        .iter()
        .filter(|part| part.id().contains("_merge"))
        .collect();
    assert_eq!(generated.len(), 1);
    assert_preserved(&original, &output, &generated);
}
