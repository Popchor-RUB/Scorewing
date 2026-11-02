mod common;
use common::*;
use scorewing_core::{
    merge::merge_score_with_stems,
    model::{frac, parse_score, Score},
};
use std::collections::HashSet;
use xmltree::Element;

fn merge(parts: &[(&str, Vec<String>)], stems: &[&str]) -> Score {
    let input = score(parts);
    let group = input
        .parts
        .iter()
        .map(|part| part.id().to_owned())
        .collect();
    let output = merge_score_with_stems(
        &input,
        &[group],
        &[stems.iter().map(|stem| (*stem).into()).collect()],
    )
    .unwrap();
    scorewing_core::validate_score(&output).unwrap();
    parse_score(output).unwrap()
}

fn direction(content: &str) -> String {
    format!("<direction><direction-type>{content}</direction-type></direction>")
}

fn dynamic(value: &str) -> String {
    direction(&format!("<dynamics><{value}/></dynamics>"))
}

fn wedge(kind: &str, number: usize) -> String {
    direction(&format!("<wedge type=\"{kind}\" number=\"{number}\"/>"))
}

fn markings(score: &Score) -> Vec<(&Element, &Element)> {
    score.parts[0]
        .measures
        .iter()
        .flat_map(|measure| elements(&measure.source))
        .filter(|node| node.name == "direction")
        .flat_map(|node| {
            elements(node)
                .filter(|kind| kind.name == "direction-type")
                .flat_map(elements)
                .filter(|symbol| ["dynamics", "wedge"].contains(&symbol.name.as_str()))
                .map(move |symbol| (node, symbol))
        })
        .collect()
}

#[test]
fn compatible_rhythms_rejoin_and_overflow_slots_stay_stable_across_bars() {
    let upper = n("C").duration(2).kind("half").xml() + &n("D").xml() + &n("E").xml();
    let other = n("F").xml() + &n("G").xml() + &n("D").xml() + &n("E").xml();
    let lower = n("A").duration(4).kind("whole").xml();
    let result = merge(
        &[
            ("S", vec![upper.clone(), upper]),
            ("T", vec![other.clone(), other]),
            ("B", vec![lower.clone(), lower]),
        ],
        &["up", "up", "down"],
    );
    for measure in &result.parts[0].measures {
        assert_eq!(
            measure
                .events
                .iter()
                .map(|event| event.voice.as_str())
                .collect::<HashSet<_>>(),
            HashSet::from(["1", "2", "3"])
        );
        assert_eq!(
            measure
                .events
                .iter()
                .filter(|event| event.voice == "3")
                .map(|event| (event.start.clone(), event.duration.clone()))
                .collect::<Vec<_>>(),
            [(frac(0), frac(1)), (frac(1), frac(1))]
        );
        assert!(measure
            .events
            .iter()
            .filter(|event| event.start >= frac(2))
            .all(|event| event.voice == "1"));
        assert_eq!(measure.events[0].duration, frac(2));
        assert_eq!(text(&measure.events[0].notes[0], "type"), "half");
    }
}

#[test]
fn duplicate_rests_keep_lowest_participating_voice_when_voice_one_sounds() {
    let rest = n("C").rest().xml();
    let result = merge(
        &[
            ("Held", vec![n("C").duration(2).kind("half").xml()]),
            ("Upper", vec![n("D").xml() + &rest]),
            ("Lower", vec![n("E").xml() + &rest]),
        ],
        &["up", "up", "down"],
    );
    let rests: Vec<_> = result.parts[0].measures[0]
        .events
        .iter()
        .filter(|event| event.notes[0].get_child("rest").is_some())
        .collect();
    assert_eq!(rests.len(), 1);
    assert_eq!(
        (
            rests[0].voice.as_str(),
            rests[0].start.clone(),
            rests[0].duration.clone()
        ),
        ("2", frac(1), frac(1))
    );
    assert_eq!(result.parts[0].measures[0].length, frac(2));
}

#[test]
fn lyrics_ignore_engraving_but_preserve_verses_syllables_and_extensions() {
    let upper = "<lyric number=\"1\" default-y=\"-20\"><text font-size=\"10\">same</text></lyric><lyric number=\"2\"><text>verse</text></lyric><lyric number=\"4\"><syllabic>begin</syllabic><text>sing</text></lyric><lyric number=\"5\"><text>long</text><extend type=\"start\"/></lyric>";
    let lower = "<lyric number=\"1\" placement=\"above\"><text font-size=\"14\">same</text></lyric><lyric number=\"3\"><text>verse</text></lyric><lyric number=\"4\"><syllabic>end</syllabic><text>sing</text></lyric><lyric number=\"5\"><text>long</text><extend type=\"continue\"/></lyric>";
    let result = merge(
        &[
            ("Upper", vec![n("C").extra(upper).xml()]),
            ("Lower", vec![n("E").extra(lower).xml()]),
        ],
        &["up", "down"],
    );
    let events = &result.parts[0].measures[0].events;
    let lyrics = |index: usize| {
        elements(&events[index].notes[0])
            .filter(|node| node.name == "lyric")
            .collect::<Vec<_>>()
    };
    let upper = lyrics(0);
    let lower = lyrics(1);
    assert_eq!(upper.len(), 4);
    assert_eq!(lower.len(), 3);
    assert!(lower.iter().all(|node| text(node, "text") != "same"));
    let last_upper = upper
        .iter()
        .map(|node| node.attributes["number"].parse::<usize>().unwrap())
        .max()
        .unwrap();
    assert!(lower
        .iter()
        .all(|node| node.attributes["number"].parse::<usize>().unwrap() > last_upper));
    assert!(upper
        .iter()
        .chain(&lower)
        .all(|node| node.attributes["placement"] == "below"));
    assert_eq!(text(upper[2], "syllabic"), "begin");
    assert_eq!(text(lower[1], "syllabic"), "end");
    assert_eq!(
        upper[3].get_child("extend").unwrap().attributes["type"],
        "start"
    );
    assert_eq!(
        lower[2].get_child("extend").unwrap().attributes["type"],
        "continue"
    );
}

#[test]
fn simultaneous_dynamics_ignore_layout_and_explicit_later_repeats_survive() {
    let upper = dynamic("p") + &n("C").xml() + &dynamic("p") + &n("D").xml();
    let lower = "<direction placement=\"below\"><direction-type><dynamics default-y=\"-40\" font-size=\"14\"><p/></dynamics></direction-type></direction>".to_owned()
        + &n("E").xml() + &dynamic("p") + &n("F").xml();
    let result = merge(
        &[("Lower", vec![lower]), ("Upper", vec![upper])],
        &["down", "up"],
    );
    let marks = markings(&result);
    assert_eq!(marks.len(), 2);
    assert!(marks.iter().all(
        |(direction, symbol)| direction.attributes["placement"] == "above"
            && text(direction, "voice") == "1"
            && symbol.get_child("p").is_some()
    ));
}

#[test]
fn direction_offsets_match_musical_positions_across_different_divisions() {
    let upper = "<direction><direction-type><dynamics><p/></dynamics></direction-type><offset>1</offset></direction>".to_owned() + &n("C").xml() + &n("D").xml();
    let lower = "<attributes><divisions>2</divisions></attributes>".to_owned()
        + &n("E").duration(2).xml()
        + &dynamic("p")
        + &n("F").duration(2).xml();
    let result = merge(&[("S", vec![upper]), ("A", vec![lower])], &["up", "down"]);
    assert_eq!(markings(&result).len(), 1);
    let extra = result.parts[0].measures[0]
        .extras
        .iter()
        .find(|extra| {
            extra
                .element
                .get_child("direction-type")
                .and_then(|kind| kind.get_child("dynamics"))
                .is_some()
        })
        .unwrap();
    assert_eq!(
        extra.position.clone()
            + scorewing_core::model::parse_fraction(&text(&extra.element, "offset")).unwrap()
                / &extra.divisions,
        frac(1)
    );
}

#[test]
fn conflicts_use_active_dynamics_across_bars_and_shared_changes_go_above() {
    let result = merge(
        &[
            (
                "Up",
                vec![
                    dynamic("p") + &n("C").xml(),
                    n("D").xml(),
                    dynamic("mf") + &n("E").xml(),
                ],
            ),
            (
                "Down",
                vec![
                    dynamic("f") + &n("G").xml(),
                    dynamic("mp") + &n("A").xml(),
                    dynamic("mf") + &n("B").xml(),
                ],
            ),
        ],
        &["up", "down"],
    );
    let marks = markings(&result);
    assert_eq!(
        marks
            .iter()
            .map(|(direction, symbol)| (
                elements(symbol).next().unwrap().name.as_str(),
                direction.attributes["placement"].as_str()
            ))
            .collect::<Vec<_>>(),
        [
            ("p", "above"),
            ("f", "below"),
            ("mp", "below"),
            ("mf", "above")
        ]
    );
}

#[test]
fn conflicting_dynamics_within_one_stem_group_are_both_preserved() {
    for stem in ["up", "down"] {
        let result = merge(
            &[
                ("A", vec![dynamic("p") + &n("C").xml()]),
                ("B", vec![dynamic("f") + &n("E").xml()]),
            ],
            &[stem, stem],
        );
        let marks = markings(&result);
        assert_eq!(marks.len(), 2);
        assert!(marks
            .iter()
            .all(|(direction, _)| direction.attributes["placement"]
                == if stem == "up" { "above" } else { "below" }));
    }
}

#[test]
fn matching_hairpins_deduplicate_complete_cross_bar_relationships() {
    let source = |number| {
        vec![
            wedge("crescendo", number) + &n("C").xml() + &wedge("continue", number) + &n("D").xml(),
            n("E").duration(2).kind("half").xml() + &wedge("stop", number),
        ]
    };
    let result = merge(&[("Down", source(7)), ("Up", source(1))], &["down", "up"]);
    let marks = markings(&result);
    assert_eq!(marks.len(), 3);
    assert_eq!(
        marks
            .iter()
            .map(|(_, symbol)| symbol.attributes["type"].as_str())
            .collect::<Vec<_>>(),
        ["crescendo", "continue", "stop"]
    );
    assert!(marks.iter().all(
        |(direction, symbol)| direction.attributes["placement"] == "above"
            && symbol.attributes["number"] == "1"
    ));
}

#[test]
fn different_hairpin_spans_retain_both_relationships_and_opposite_placements() {
    let upper = wedge("crescendo", 1)
        + &n("C").duration(2).kind("half").xml()
        + &wedge("stop", 1)
        + &n("D").duration(2).kind("half").xml();
    let lower = wedge("crescendo", 1)
        + &n("E").xml()
        + &wedge("stop", 1)
        + &n("F").duration(3).kind("half").extra("<dot/>").xml();
    let result = merge(
        &[("Up", vec![upper]), ("Down", vec![lower])],
        &["up", "down"],
    );
    let marks = markings(&result);
    assert_eq!(marks.len(), 4);
    for (direction, symbol) in marks {
        assert_eq!(
            direction.attributes["placement"],
            if symbol.attributes["number"] == "1" {
                "above"
            } else {
                "below"
            }
        );
    }
}

#[test]
fn dynamic_deduplication_keeps_words_and_distinct_playback_payloads() {
    let source = |velocity| {
        format!("<direction><direction-type><words>dolce</words></direction-type><direction-type><dynamics><p/></dynamics></direction-type><sound dynamics=\"{velocity}\"/></direction>") + &n("C").xml()
    };
    let result = merge(
        &[("Up", vec![source(40)]), ("Down", vec![source(60)])],
        &["up", "down"],
    );
    assert_eq!(markings(&result).len(), 1);
    let nodes = descendants(&result.parts[0].source);
    assert_eq!(
        nodes
            .iter()
            .filter(|node| node.name == "words" && node.get_text().as_deref() == Some("dolce"))
            .count(),
        2
    );
    assert_eq!(
        nodes
            .iter()
            .filter(|node| node.name == "sound")
            .map(|node| node.attributes["dynamics"].as_str())
            .collect::<HashSet<_>>(),
        HashSet::from(["40", "60"])
    );
}

#[test]
fn note_attached_dynamics_follow_the_same_placement_rules() {
    let note = |step, symbol| {
        n(step).extra(&format!("<notations><dynamics><{symbol}/></dynamics><articulations><accent/></articulations></notations>")).xml()
    };
    let result = merge(
        &[("Up", vec![note("C", "p")]), ("Down", vec![note("E", "f")])],
        &["up", "down"],
    );
    let marks = markings(&result);
    assert_eq!(marks.len(), 2);
    assert_eq!(marks[0].0.attributes["placement"], "above");
    assert_eq!(marks[1].0.attributes["placement"], "below");
    assert_eq!(
        descendants(&result.parts[0].source)
            .iter()
            .filter(|node| node.name == "accent")
            .count(),
        2
    );
}

#[test]
fn lyric_font_runs_and_default_syllabification_do_not_create_duplicates() {
    let upper = "<lyric><text>hello</text></lyric>";
    let lower = "<lyric number=\"1\"><syllabic>single</syllabic><text font-weight=\"bold\">hel</text><text>lo</text><end-line/></lyric>";
    let result = merge(
        &[
            ("Up", vec![n("C").extra(upper).xml()]),
            ("Down", vec![n("E").extra(lower).xml()]),
        ],
        &["up", "down"],
    );
    assert_eq!(
        descendants(&result.parts[0].source)
            .iter()
            .filter(|node| node.name == "lyric")
            .count(),
        1
    );
}

#[test]
fn hairpin_continuation_layout_does_not_change_span_identity() {
    let upper = wedge("crescendo", 1)
        + &n("C").xml()
        + &wedge("continue", 1)
        + &n("D").xml()
        + &wedge("stop", 1);
    let lower = wedge("crescendo", 1) + &n("E").duration(2).kind("half").xml() + &wedge("stop", 1);
    let result = merge(
        &[("Up", vec![upper]), ("Down", vec![lower])],
        &["up", "down"],
    );
    let marks = markings(&result);
    assert_eq!(marks.len(), 3);
    assert!(marks.iter().all(
        |(direction, symbol)| direction.attributes["placement"] == "above"
            && symbol.attributes["number"] == "1"
    ));
}

#[test]
fn tied_extra_voice_keeps_its_number_when_other_sources_fall_silent() {
    let start = n("F")
        .duration(2)
        .kind("half")
        .xml()
        .replace("<voice>", "<tie type=\"start\"/><voice>");
    let stop = n("F")
        .duration(4)
        .kind("whole")
        .xml()
        .replace("<voice>", "<tie type=\"stop\"/><voice>");
    let result = merge(
        &[
            (
                "Held",
                vec![n("C").duration(4).kind("whole").xml(), forward(4)],
            ),
            (
                "Moving",
                vec![n("E").duration(2).kind("half").xml() + &start, stop],
            ),
        ],
        &["up", "up"],
    );
    let tied: Vec<_> = result.parts[0]
        .measures
        .iter()
        .flat_map(|measure| &measure.events)
        .filter(|event| {
            event
                .notes
                .iter()
                .any(|note| note.get_child("tie").is_some())
        })
        .collect();
    assert_eq!(tied.len(), 2);
    assert!(tied.iter().all(|event| event.voice == "3"));
    assert_eq!(tied[1].duration, frac(4));
}

#[test]
fn a_shared_tied_chord_can_diverge_into_independent_rhythms() {
    let tied = |step, duration, kind, tie| {
        n(step)
            .duration(duration)
            .kind(kind)
            .xml()
            .replace("<voice>", &format!("<tie type=\"{tie}\"/><voice>"))
    };
    let result = merge(
        &[
            (
                "Up A",
                vec![tied("C", 2, "half", "start"), tied("C", 2, "half", "stop")],
            ),
            (
                "Up B",
                vec![
                    tied("E", 2, "half", "start"),
                    tied("E", 1, "quarter", "stop") + &n("F").xml(),
                ],
            ),
        ],
        &["up", "up"],
    );
    assert_eq!(result.parts[0].measures[0].events.len(), 1);
    assert_eq!(result.parts[0].measures[0].events[0].notes.len(), 2);
    let events = &result.parts[0].measures[1].events;
    assert_eq!(
        events
            .iter()
            .map(|event| event.voice.as_str())
            .collect::<HashSet<_>>(),
        HashSet::from(["1", "3"])
    );
    assert_eq!(
        events
            .iter()
            .flat_map(|event| &event.notes)
            .filter(|note| note.get_child("tie").is_some())
            .count(),
        2
    );
}

#[test]
fn symbols_and_hairpins_in_different_groups_get_distinct_placements() {
    let upper = wedge("crescendo", 1) + &n("C").xml() + &n("D").xml() + &wedge("stop", 1);
    let lower = n("E").xml() + &dynamic("p") + &n("F").xml();
    let result = merge(
        &[("Up", vec![upper]), ("Down", vec![lower])],
        &["up", "down"],
    );
    let marks = markings(&result);
    assert!(marks
        .iter()
        .all(|(direction, symbol)| direction.attributes["placement"]
            == if symbol.name == "wedge" {
                "above"
            } else {
                "below"
            }));
    let upper = dynamic("p") + &n("C").xml() + &n("D").xml();
    let lower = n("E").xml() + &wedge("crescendo", 1) + &n("F").xml() + &wedge("stop", 1);
    let result = merge(
        &[("Up", vec![upper]), ("Down", vec![lower])],
        &["up", "down"],
    );
    let marks = markings(&result);
    assert!(marks
        .iter()
        .all(|(direction, symbol)| direction.attributes["placement"]
            == if symbol.name == "wedge" {
                "below"
            } else {
                "above"
            }));
}

#[test]
fn removing_a_shared_rest_frees_the_primary_voice_for_sounding_notes() {
    let rest = n("C").rest().xml();
    let result = merge(
        &[
            ("Up rest", vec![rest.clone()]),
            ("Down rest", vec![rest]),
            ("Down note", vec![n("E").xml()]),
        ],
        &["up", "down", "down"],
    );
    let events = &result.parts[0].measures[0].events;
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].voice, "1");
    assert!(events[0].notes[0].get_child("rest").is_some());
    assert_eq!(events[1].voice, "2");
    assert_eq!(text(&events[1].notes[0], "pitch/step"), "E");
}

#[test]
fn rest_compaction_keeps_graces_before_their_principal_notes() {
    let rest = n("C").rest().xml();
    let result = merge(
        &[
            ("Up", vec![n("D").grace().xml() + &n("C").xml() + &rest]),
            ("Down", vec![n("E").xml() + &rest]),
        ],
        &["up", "down"],
    );
    let events = &result.parts[0].measures[0].events;
    assert!(events[0].grace());
    assert_eq!(events[0].start, frac(0));
    assert!(!events[1].grace());
    assert_eq!(events[1].start, frac(0));
}
