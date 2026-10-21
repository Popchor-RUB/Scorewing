//! Separate explicit voices and chord tones without guessing melodic lines.

pub(crate) use crate::model::compare_voices as voice_cmp;
use crate::model::{
    build_score, insert_note_child, render_measure, rest_event, set_child, slice_event,
    strip_position, Event, Fraction, Measure, Part, Score, TimedElement,
};
pub(crate) use crate::xml::{append, child_text, elements, visit_mut as walk_mut};
use crate::{Result, ScoreError};
use num_bigint::BigInt;
use num_traits::Zero;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use xmltree::{Element, XMLNode};

pub(crate) fn elements_mut(element: &mut Element) -> impl Iterator<Item = &mut Element> {
    element
        .children
        .iter_mut()
        .filter_map(XMLNode::as_mut_element)
}

pub(crate) fn descendants(element: &Element) -> Vec<&Element> {
    let mut result = vec![element];
    for child in elements(element) {
        result.extend(descendants(child));
    }
    result
}

pub(crate) fn remove_children(element: &mut Element, names: &[&str]) {
    element.children.retain(|node| {
        node.as_element()
            .is_none_or(|e| !names.contains(&e.name.as_str()))
    });
}

pub(crate) fn numeric(text: &str) -> Result<Fraction> {
    crate::model::parse_fraction(text)
}

fn pitch(note: &Element) -> Result<Option<(Fraction, BigInt, usize)>> {
    let (step, octave, alter) = if let Some(pitch) = note.get_child("pitch") {
        (
            child_text(pitch, "step"),
            child_text(pitch, "octave"),
            child_text(pitch, "alter").unwrap_or_else(|| "0".into()),
        )
    } else if let Some(pitch) = note.get_child("unpitched") {
        (
            Some(child_text(pitch, "display-step").unwrap_or_else(|| "C".into())),
            Some(child_text(pitch, "display-octave").unwrap_or_else(|| "4".into())),
            "0".into(),
        )
    } else {
        return Ok(None);
    };
    let invalid = || ScoreError::new("Cannot split an invalid pitch");
    let step = step.ok_or_else(invalid)?;
    let index = ["C", "D", "E", "F", "G", "A", "B"]
        .iter()
        .position(|s| *s == step)
        .ok_or_else(invalid)?;
    let octave: BigInt = octave.ok_or_else(invalid)?.parse().map_err(|_| invalid())?;
    let alter = numeric(&alter).map_err(|_| invalid())?;
    Ok(Some((
        Fraction::from_integer(&octave * 12 + [0, 2, 4, 5, 7, 9, 11][index]) + alter,
        octave,
        index,
    )))
}

fn notes<'a>(event: &'a Event, tone: usize, mode: &str, context: &str) -> Result<Vec<&'a Element>> {
    if mode == "voices" || event.notes.len() == 1 {
        return Ok(event.notes.iter().collect());
    }
    let mut keyed = Vec::new();
    for note in &event.notes {
        let pitch = pitch(note)?.ok_or_else(|| {
            ScoreError::new(format!(
                "{context}, position {}: a chord cannot contain rests",
                event.start
            ))
        })?;
        keyed.push((pitch, note));
    }
    keyed.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(keyed
        .get(tone)
        .map(|(_, note)| vec![*note])
        .unwrap_or_default())
}

fn segments(measure: &Measure, voice: &str) -> Vec<(Fraction, Fraction, Option<usize>)> {
    let mut boundaries = BTreeSet::from([Fraction::zero(), measure.length.clone()]);
    for event in measure.events.iter().filter(|event| !event.grace()) {
        boundaries.extend([event.start.clone(), event.end()]);
    }
    let boundaries: Vec<_> = boundaries.into_iter().collect();
    let mut result: Vec<(Fraction, Fraction, Option<usize>)> = Vec::new();
    for pair in boundaries.windows(2) {
        let start = &pair[0];
        let mut active = BTreeMap::new();
        for (index, event) in measure
            .events
            .iter()
            .enumerate()
            .filter(|(_, event)| !event.grace())
        {
            if event.start <= *start && *start < event.end() {
                active.insert(event.voice.as_str(), index);
            }
        }
        let event = active
            .get(voice)
            .copied()
            .or_else(|| (active.len() == 1).then(|| *active.values().next().unwrap()));
        if let Some(last) = result.last_mut().filter(|last| last.2 == event) {
            last.1 = pair[1].clone();
        } else {
            result.push((start.clone(), pair[1].clone(), event));
        }
    }
    result
}

fn inherit_chord_marks(primary: &Element, note: &mut Element) {
    let verses: HashSet<_> = elements(note)
        .filter(|e| e.name == "lyric")
        .map(|e| {
            e.attributes
                .get("number")
                .map(String::as_str)
                .unwrap_or("1")
                .to_owned()
        })
        .collect();
    for lyric in elements(primary).filter(|e| e.name == "lyric") {
        if !verses.contains(
            lyric
                .attributes
                .get("number")
                .map(String::as_str)
                .unwrap_or("1"),
        ) {
            insert_note_child(note, lyric.clone());
        }
    }
    for tag in ["beam", "time-modification"] {
        if note.get_child(tag).is_none() {
            for child in elements(primary).filter(|e| e.name == tag) {
                insert_note_child(note, child.clone());
            }
        }
    }
    for tag in ["tuplet", "articulations"] {
        if note
            .get_child("notations")
            .and_then(|n| n.get_child(tag))
            .is_some()
        {
            continue;
        }
        if let Some(primary_notations) = primary.get_child("notations") {
            for child in elements(primary_notations).filter(|e| e.name == tag) {
                if note.get_child("notations").is_none() {
                    insert_note_child(note, Element::new("notations"));
                }
                append(note.get_mut_child("notations").unwrap(), child.clone());
            }
        }
    }
}

type Assignments = HashMap<usize, BTreeSet<usize>>;

fn assign(assignments: &mut Assignments, note: &Element, lane: usize) {
    assignments
        .entry(note as *const Element as usize)
        .or_default()
        .insert(lane);
}

fn lane_measure(
    measure: &Measure,
    voice: &str,
    tone: usize,
    mode: &str,
    assignments: &mut Assignments,
    lane: usize,
) -> Result<Element> {
    let mut result = Vec::new();
    for (start, end, index) in segments(measure, voice) {
        let source = index.map(|index| &measure.events[index]);
        let selected_notes = match source {
            Some(event) => notes(event, tone, mode, &measure.context)?,
            None => Vec::new(),
        };
        if selected_notes.is_empty() {
            result.push(rest_event(start.clone(), &end - &start, "1")?);
            continue;
        }
        let source = source.unwrap();
        let selected = Event {
            start: source.start.clone(),
            duration: source.duration.clone(),
            notes: selected_notes.iter().map(|n| (*n).clone()).collect(),
            voice: "1".into(),
        };
        let sliced = start != source.start || end != source.end();
        let mut event = if sliced {
            slice_event(&selected, start.clone(), end.clone())?
        } else {
            selected
        };
        for (original, copied) in selected_notes.iter().zip(&mut event.notes) {
            assign(assignments, original, lane);
            if sliced {
                let keep: HashSet<_> = elements(original)
                    .filter(|e| e.name == "tie")
                    .filter_map(|tie| tie.attributes.get("type"))
                    .filter(|kind| {
                        (*kind == "start" && end == source.end())
                            || (*kind == "stop" && start == source.start)
                    })
                    .cloned()
                    .collect();
                copied.children.retain(|node| {
                    node.as_element().is_none_or(|e| {
                        e.name != "tie"
                            || e.attributes
                                .get("type")
                                .is_some_and(|kind| keep.contains(kind))
                    })
                });
                if let Some(notations) = copied.get_mut_child("notations") {
                    notations.children.retain(|node| {
                        node.as_element().is_none_or(|e| {
                            e.name != "tied"
                                || e.attributes
                                    .get("type")
                                    .is_some_and(|kind| keep.contains(kind))
                        })
                    });
                }
            }
            if mode == "chords" && source.notes.len() > 1 && start == source.start {
                inherit_chord_marks(&source.notes[0], copied);
            }
        }
        result.push(event);
    }
    for source in measure.events.iter().filter(|event| event.grace()) {
        let occupied: HashSet<_> = measure
            .events
            .iter()
            .filter(|e| {
                (e.start <= source.start && source.start < e.end())
                    || (e.grace() && e.start == source.start)
            })
            .map(|e| e.voice.as_str())
            .collect();
        if source.voice != voice && occupied.len() != 1 {
            continue;
        }
        let selected = notes(source, tone, mode, &measure.context)?;
        if selected.is_empty() {
            continue;
        }
        let mut copied: Vec<Element> = selected.iter().map(|n| (*n).clone()).collect();
        if mode == "chords" && source.notes.len() > 1 {
            for node in &mut copied {
                inherit_chord_marks(&source.notes[0], node);
            }
        }
        result.push(Event {
            start: source.start.clone(),
            duration: Fraction::zero(),
            notes: copied,
            voice: "1".into(),
        });
        for original in selected {
            assign(assignments, original, lane);
        }
    }
    result.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| b.grace().cmp(&a.grace()))
    });
    let mut extras = Vec::new();
    for extra in &measure.extras {
        if let Some(source_voice) = child_text(&extra.element, "voice") {
            let occupied: HashSet<_> = measure
                .events
                .iter()
                .filter(|e| e.start <= extra.position && extra.position < e.end())
                .map(|e| e.voice.as_str())
                .collect();
            if source_voice != voice
                && !(occupied.len() == 1 && occupied.contains(source_voice.as_str()))
            {
                continue;
            }
            let mut node = extra.element.clone();
            set_child(&mut node, "voice", "1");
            extras.push(TimedElement {
                position: extra.position.clone(),
                element: node,
                divisions: extra.divisions.clone(),
            });
        } else {
            extras.push(extra.clone());
        }
    }
    render_measure(measure, &result, Some(&extras), None, None)
}

fn check_relationships(part: &Part, assignments: &Assignments) -> Result<()> {
    let mut ties = HashMap::new();
    let mut slurs = HashMap::new();
    for measure in &part.measures {
        let mut events: Vec<_> = measure.events.iter().collect();
        events.sort_by(|a, b| a.start.cmp(&b.start));
        for event in events {
            for note in &event.notes {
                let lanes = assignments
                    .get(&(note as *const Element as usize))
                    .cloned()
                    .unwrap_or_default();
                let pitch = note
                    .get_child("pitch")
                    .map(|p| {
                        Ok::<_, ScoreError>((
                            child_text(p, "step"),
                            numeric(&child_text(p, "alter").unwrap_or_else(|| "0".into()))?,
                            child_text(p, "octave"),
                        ))
                    })
                    .transpose()?;
                let key = (event.voice.clone(), pitch);
                let mut kinds: HashSet<_> = elements(note)
                    .filter(|e| e.name == "tie")
                    .filter_map(|e| e.attributes.get("type"))
                    .map(String::as_str)
                    .collect();
                if let Some(notations) = note.get_child("notations") {
                    kinds.extend(
                        elements(notations)
                            .filter(|e| e.name == "tied")
                            .filter_map(|e| e.attributes.get("type"))
                            .map(String::as_str),
                    );
                }
                if kinds.contains("stop")
                    && ties.remove(&key).is_some_and(|previous| previous != lanes)
                {
                    return Err(ScoreError::new(format!(
                        "{}, position {}: ambiguous tie assignment when splitting",
                        measure.context, event.start
                    )));
                }
                if kinds.contains("start") {
                    ties.insert(key, lanes.clone());
                }
                if let Some(notations) = note.get_child("notations") {
                    for slur in elements(notations).filter(|e| e.name == "slur") {
                        let key = (
                            event.voice.clone(),
                            slur.attributes
                                .get("number")
                                .map(String::as_str)
                                .unwrap_or("1")
                                .to_owned(),
                        );
                        let kind = slur.attributes.get("type").map(String::as_str);
                        if kind == Some("stop")
                            && slurs.remove(&key).is_some_and(|previous| previous != lanes)
                        {
                            return Err(ScoreError::new(format!(
                                "{}, position {}: ambiguous slur assignment when splitting",
                                measure.context, event.start
                            )));
                        }
                        if kind == Some("start") {
                            slurs.insert(key, lanes.clone());
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

pub(crate) const ID_REFERENCES: &[&str] = &[
    "part",
    "instrument",
    "midi-instrument",
    "midi-device",
    "play",
    "instrument-change",
    "instrument-link",
];

fn copy_ids(
    definition: &mut Element,
    part: &mut Element,
    new_id: &str,
    used: &mut HashSet<String>,
) {
    let mut mapping = HashMap::from([(
        definition.attributes.get("id").unwrap().clone(),
        new_id.to_owned(),
    )]);
    for root in [&*definition, &*part] {
        for element in descendants(root) {
            if let Some(old) = element.attributes.get("id") {
                if !ID_REFERENCES.contains(&element.name.as_str()) && !mapping.contains_key(old) {
                    let mut candidate = format!("{new_id}_{}", mapping.len());
                    while used.contains(&candidate) {
                        candidate.push('_');
                    }
                    used.insert(candidate.clone());
                    mapping.insert(old.clone(), candidate);
                }
            }
        }
    }
    let mut occurrences: HashMap<String, usize> = HashMap::new();
    for root in [definition, part] {
        walk_mut(root, &mut |element| {
            for (key, value) in &mut element.attributes {
                if key == "id" {
                    if let Some(renamed) = mapping.get(value) {
                        let mut renamed = renamed.clone();
                        if !ID_REFERENCES.contains(&element.name.as_str()) {
                            let count = occurrences.entry(value.clone()).or_default();
                            *count += 1;
                            if *count > 1 {
                                renamed.push_str(&format!("_occurrence_{count}"));
                                while used.contains(&renamed) {
                                    renamed.push('_');
                                }
                                used.insert(renamed.clone());
                            }
                        }
                        *value = renamed;
                    }
                } else if key == "player" {
                    if let Some(renamed) = mapping.get(value) {
                        *value = renamed.clone();
                    }
                } else if key.rsplit(['}', ':']).next() == Some("href") && value.starts_with('#') {
                    if let Some(renamed) = mapping.get(&value[1..]) {
                        *value = format!("#{renamed}");
                    }
                }
            }
        });
        strip_position(root);
    }
}

/// Retain selected originals and insert their derived lanes directly below them.
/// `None` preserves the CLI's split-all default; selections use stable part IDs.
pub fn split_score(score: &Score, mode: &str, selected: Option<&[String]>) -> Result<Element> {
    if !["chords", "voices"].contains(&mode) {
        return Err(ScoreError::new("Split mode must be 'chords' or 'voices'"));
    }
    if let Some(selected) = selected {
        if selected.is_empty() {
            return Err(ScoreError::new("Select at least one part to split"));
        }
        let mut seen = HashSet::new();
        for id in selected {
            if !score.parts.iter().any(|part| part.id() == id) {
                return Err(ScoreError::new(format!("Unknown part ID {id:?}")));
            }
            if !seen.insert(id) {
                return Err(ScoreError::new(format!("Repeated part ID {id:?}")));
            }
        }
    }
    let mut used: HashSet<_> = descendants(&score.root)
        .iter()
        .filter_map(|node| node.attributes.get("id").cloned())
        .collect();
    let mut replacements = HashMap::new();
    for part in &score.parts {
        if selected.is_some_and(|ids| !ids.iter().any(|id| id == part.id())) {
            continue;
        }
        let mut widths: HashMap<&str, usize> = HashMap::new();
        for measure in &part.measures {
            for event in &measure.events {
                let width = if mode == "chords" {
                    event.notes.len()
                } else {
                    1
                };
                widths
                    .entry(&event.voice)
                    .and_modify(|n| *n = (*n).max(width))
                    .or_insert(width.max(1));
            }
        }
        let mut voices: Vec<_> = widths.keys().copied().collect();
        voices.sort_by(|a, b| voice_cmp(a, b));
        let slots: Vec<_> = voices
            .iter()
            .flat_map(|voice| (0..widths[voice]).map(move |tone| (*voice, tone)))
            .collect();
        if slots.len() < 2 {
            continue;
        }
        let mut assignments = HashMap::new();
        let mut copies = vec![(part.definition.clone(), part.source.clone())];
        for (index, (voice, tone)) in slots.iter().enumerate() {
            let lane = index + 1;
            let mut new_id = format!("{}_split_{lane}", part.id());
            while used.contains(&new_id) {
                new_id.push('_');
            }
            used.insert(new_id.clone());
            let mut definition = part.definition.clone();
            set_child(
                &mut definition,
                "part-name",
                format!("{} {lane}", part.name()),
            );
            remove_children(
                &mut definition,
                &["part-name-display", "part-abbreviation-display"],
            );
            if let Some(abbreviation) = child_text(&definition, "part-abbreviation") {
                let name = if abbreviation.is_empty() {
                    part.name()
                } else {
                    abbreviation
                };
                set_child(
                    &mut definition,
                    "part-abbreviation",
                    format!("{name} {lane}"),
                );
            }
            let mut derived = Element::new("part");
            derived.attributes = part.source.attributes.clone();
            for measure in &part.measures {
                append(
                    &mut derived,
                    lane_measure(measure, voice, *tone, mode, &mut assignments, lane)?,
                );
            }
            copy_ids(&mut definition, &mut derived, &new_id, &mut used);
            copies.push((definition, derived));
        }
        check_relationships(part, &assignments)?;
        replacements.insert(part.id().to_owned(), copies);
    }
    Ok(build_score(score, &replacements))
}
