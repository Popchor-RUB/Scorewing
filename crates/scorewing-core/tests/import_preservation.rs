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

fn assert_preserved(original: &Score, output: &Score, generated: &[&Part]) {
    for part in &original.parts {
        let retained = output
            .parts
            .iter()
            .find(|retained| retained.id() == part.id())
            .unwrap();
        assert_eq!(retained.source, part.source);
        assert_eq!(retained.definition, part.definition);
        assert_eq!(retained.visible(), part.visible());
    }
    assert!(!generated.is_empty());
    for part in generated {
        assert!(part.visible());
        assert!(part.supported());
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
fn split_keeps_all_originals_and_makes_hidden_source_results_visible_with_global_marks() {
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
fn merge_keeps_all_originals_and_makes_hidden_source_result_visible_with_global_marks() {
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
