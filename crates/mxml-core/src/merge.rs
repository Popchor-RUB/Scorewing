//! Merge selected single-staff parts into ordered, stem-directed voices.

use crate::model::{
    build_score, insert_note_child, render_measure, rest_event, set_child, slice_event, Event,
    Fraction, Measure, Part, Score, TimedElement,
};
use crate::split::{
    append, child_text, descendants, elements, elements_mut, numeric, remove_children, voice_cmp,
    walk_mut, ID_REFERENCES,
};
use crate::{check_errors, Result, ScoreError};
use num_bigint::BigInt;
use num_traits::Zero;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use xmltree::{Element, XMLNode};

fn canonical(element: &Element) -> String {
    fn field(result: &mut String, value: &str) {
        result.push_str(&format!("{}:{value}", value.len()));
    }
    fn visit(element: &Element, result: &mut String) {
        result.push('E');
        field(result, element.namespace.as_deref().unwrap_or_default());
        field(result, &element.name);
        let attributes: BTreeMap<_, _> = element.attributes.iter().collect();
        for (key, value) in attributes {
            result.push('A');
            field(result, key);
            field(result, value);
        }
        let text: String = element
            .children
            .iter()
            .take_while(|node| matches!(node, XMLNode::Text(_) | XMLNode::CData(_)))
            .filter_map(|node| match node {
                XMLNode::Text(text) | XMLNode::CData(text) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        if !text.trim().is_empty() {
            result.push('T');
            field(result, &text);
        }
        for node in &element.children {
            match node {
                XMLNode::Element(child) => visit(child, result),
                XMLNode::Comment(text) => {
                    result.push('C');
                    field(result, text);
                }
                XMLNode::ProcessingInstruction(name, value) => {
                    result.push('P');
                    field(result, name);
                    field(result, value.as_deref().unwrap_or_default());
                }
                _ => {}
            }
        }
        result.push('/');
    }
    let mut result = String::new();
    visit(element, &mut result);
    result
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Pitch {
    Rest,
    Pitched(String, Fraction, BigInt),
    Unpitched(Option<String>),
}

fn pitch(note: &Element) -> Result<Pitch> {
    if note.get_child("rest").is_some() {
        return Ok(Pitch::Rest);
    }
    if let Some(pitch) = note.get_child("pitch") {
        let octave = child_text(pitch, "octave")
            .unwrap_or_default()
            .parse()
            .map_err(|_| ScoreError::new("Invalid pitch octave"))?;
        return Ok(Pitch::Pitched(
            child_text(pitch, "step").unwrap_or_default(),
            numeric(&child_text(pitch, "alter").unwrap_or_else(|| "0".into()))?,
            octave,
        ));
    }
    Ok(Pitch::Unpitched(note.get_child("unpitched").map(canonical)))
}

type TieState = (Vec<String>, Vec<String>);

fn tie_state(note: &Element) -> TieState {
    let mut sounding: Vec<_> = elements(note)
        .filter(|n| n.name == "tie")
        .map(|n| n.attributes.get("type").cloned().unwrap_or_default())
        .collect();
    let mut visual: Vec<_> = note
        .get_child("notations")
        .map(|n| {
            elements(n)
                .filter(|n| n.name == "tied")
                .map(|n| n.attributes.get("type").cloned().unwrap_or_default())
                .collect()
        })
        .unwrap_or_default();
    sounding.sort();
    visual.sort();
    (sounding, visual)
}

fn signature(event: &Event) -> Result<Vec<(Pitch, TieState)>> {
    let mut result: Vec<_> = event
        .notes
        .iter()
        .map(|note| Ok((pitch(note)?, tie_state(note))))
        .collect::<Result<_>>()?;
    result.sort();
    Ok(result)
}

fn select_groups(score: &Score, groups: &[Vec<String>]) -> Result<Vec<Vec<usize>>> {
    if groups.is_empty() {
        return Err(ScoreError::new(
            "merge requires at least one --pair containing two or more staves",
        ));
    }
    let mut selected = HashSet::new();
    let mut result = Vec::new();
    for group in groups {
        let mut indices = Vec::new();
        for id in group {
            let index = score
                .parts
                .iter()
                .position(|part| part.id() == id)
                .ok_or_else(|| ScoreError::new(format!("Unknown part ID {id:?}")))?;
            if indices.contains(&index) {
                return Err(ScoreError::new(format!(
                    "Repeated stave {id:?} in merge group"
                )));
            }
            if selected.contains(&index) {
                return Err(ScoreError::new(format!(
                    "Overlapping merge groups: stave {id:?} appears more than once"
                )));
            }
            indices.push(index);
        }
        if indices.len() < 2 {
            return Err(ScoreError::new(
                "A merge group must contain two or more distinct staves",
            ));
        }
        selected.extend(indices.iter().copied());
        result.push(indices);
    }
    Ok(result)
}

fn source_context(measure: &Measure, event: &Event) -> String {
    measure
        .events
        .iter()
        .filter(|source| {
            if event.grace() {
                source.grace() && source.start == event.start
            } else {
                !source.grace() && source.start < event.end() && source.end() > event.start
            }
        })
        .map(|source| source.context(measure))
        .collect::<Vec<_>>()
        .join("; ")
}

fn flatten(measure: &Measure, voice: &str) -> Result<Vec<Event>> {
    let mut errors = Vec::new();
    let mut by_span = Vec::<Event>::new();
    let mut indices = HashMap::new();
    let mut grace_counts = HashMap::new();
    for event in &measure.events {
        let mut invalid = false;
        for note in &event.notes {
            if let Err(error) = pitch(note) {
                errors.push(ScoreError::new(format!(
                    "{}: {error}",
                    event.context(measure)
                )));
                invalid = true;
            }
        }
        if invalid {
            continue;
        }
        let ordinal = if event.grace() {
            let count = grace_counts
                .entry((event.start.clone(), event.voice.clone()))
                .or_insert(0_usize);
            let ordinal = *count;
            *count += 1;
            ordinal
        } else {
            0
        };
        let key = (
            event.start.clone(),
            event.duration.clone(),
            event.grace(),
            ordinal,
        );
        if let Some(index) = indices.get(&key) {
            let index: usize = *index;
            by_span[index].notes.extend(event.notes.clone());
        } else {
            indices.insert(key, by_span.len());
            let mut event = event.clone();
            event.voice = voice.into();
            by_span.push(event);
        }
    }
    by_span.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| b.grace().cmp(&a.grace()))
    });
    let mut end = Fraction::zero();
    let mut completed = Vec::new();
    for mut event in by_span {
        if event.grace() {
            completed.push(event);
            continue;
        }
        if event.start < end {
            let context = source_context(measure, &event);
            errors.push(ScoreError::new(format!("{context}: incompatible overlapping rhythmic spans cannot be flattened into one voice")));
        }
        if event.start > end {
            completed.push(rest_event(end.clone(), &event.start - &end, voice)?);
        }
        let sounded: Vec<_> = event
            .notes
            .iter()
            .filter(|n| n.get_child("rest").is_none())
            .cloned()
            .collect();
        event.notes = if sounded.is_empty() {
            event.notes.into_iter().take(1).collect()
        } else {
            sounded
        };
        let mut unique = Vec::<Element>::new();
        let mut keys = HashMap::new();
        for note in event.notes {
            let key = (pitch(&note)?, tie_state(&note));
            if let Some(index) = keys.get(&key) {
                combine_note(&mut unique[*index], &note);
            } else {
                keys.insert(key, unique.len());
                unique.push(note);
            }
        }
        event.notes = unique;
        end = end.max(event.end());
        completed.push(event);
    }
    if end < measure.length {
        completed.push(rest_event(end.clone(), &measure.length - &end, voice)?);
    }
    check_errors(errors)?;
    Ok(completed)
}

fn lyric_identity(lyric: &Element) -> String {
    let mut identity = lyric.clone();
    identity.attributes.remove("number");
    identity.attributes.remove("name");
    canonical(&identity)
}

fn combine_note(target: &mut Element, source: &Element) {
    for lyric in elements(source).filter(|n| n.name == "lyric") {
        let identity = lyric_identity(lyric);
        if !elements(target)
            .filter(|n| n.name == "lyric")
            .any(|n| lyric_identity(n) == identity)
        {
            insert_note_child(target, lyric.clone());
        }
    }
    let Some(other) = source.get_child("notations") else {
        return;
    };
    if target.get_child("notations").is_none() {
        insert_note_child(target, other.clone());
        return;
    }
    let own = target.get_mut_child("notations").unwrap();
    for annotation in elements(other) {
        if ["articulations", "ornaments", "technical"].contains(&annotation.name.as_str())
            && own.get_child(annotation.name.as_str()).is_some()
        {
            let same = own.get_mut_child(annotation.name.as_str()).unwrap();
            for item in elements(annotation) {
                if !elements(same).any(|n| canonical(n) == canonical(item)) {
                    append(same, item.clone());
                }
            }
        } else if !elements(own)
            .filter(|n| n.name == annotation.name)
            .any(|n| canonical(n) == canonical(annotation))
        {
            append(own, annotation.clone());
        }
    }
}

type LyricLines = HashMap<(usize, String), String>;

fn lyric_lines(parts: &[&Part]) -> LyricLines {
    let mut mapping = HashMap::new();
    for (source, part) in parts.iter().enumerate() {
        let mut verses: Vec<_> = part
            .measures
            .iter()
            .flat_map(|m| &m.events)
            .flat_map(|e| &e.notes)
            .flat_map(elements)
            .filter(|n| n.name == "lyric")
            .map(|n| {
                n.attributes
                    .get("number")
                    .cloned()
                    .unwrap_or_else(|| "1".into())
            })
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        verses.sort_by(|a, b| voice_cmp(a, b));
        for verse in verses {
            let number = (mapping.len() + 1).to_string();
            mapping.insert((source, verse), number);
        }
    }
    mapping
}

fn prepare(
    events: &mut [Event],
    source: usize,
    stem: &str,
    lines: &LyricLines,
    relationships: &Relationships,
    default_instrument: Option<&str>,
) {
    for event in events {
        for note in &mut event.notes {
            renumber_relationships(note, source, relationships);
            if let Some(id) = default_instrument.filter(|_| note.get_child("instrument").is_none())
            {
                let mut instrument = Element::new("instrument");
                instrument.attributes.insert("id".into(), id.into());
                insert_note_child(note, instrument);
            }
            for lyric in elements_mut(note).filter(|n| n.name == "lyric") {
                let verse = lyric
                    .attributes
                    .get("number")
                    .cloned()
                    .unwrap_or_else(|| "1".into());
                lyric
                    .attributes
                    .insert("number".into(), lines[&(source, verse)].clone());
            }
            let stemless = note.get_child("rest").is_some()
                || ["whole", "breve", "long", "maxima"]
                    .contains(&child_text(note, "type").as_deref().unwrap_or_default());
            if stemless {
                remove_children(note, &["stem"]);
            } else if child_text(note, "stem").as_deref() != Some("none") {
                set_child(note, "stem", stem);
            }
        }
    }
}

fn state_signature(tag: &str, element: Option<&Element>) -> Result<Option<String>> {
    if tag == "transpose" {
        let mut result = String::new();
        for tag in ["diatonic", "chromatic", "octave-change"] {
            let value = element
                .and_then(|e| child_text(e, tag))
                .unwrap_or_else(|| "0".into());
            let value =
                numeric(&value).map_err(|_| ScoreError::new("Invalid numeric transposition"))?;
            result.push_str(&format!("{value};"));
        }
        let double = element.and_then(|e| e.get_child("double"));
        result.push_str(&format!(
            "{};{:?}",
            double.is_some(),
            double.map(|e| e
                .attributes
                .get("above")
                .map(String::as_str)
                .unwrap_or("no"))
        ));
        return Ok(Some(result));
    }
    let Some(element) = element else {
        return Ok(None);
    };
    let mut value = element.clone();
    walk_mut(&mut value, &mut |node| {
        let number = if node.name == "key-octave" {
            node.attributes.get("number").cloned()
        } else {
            None
        };
        node.attributes.clear();
        if let Some(number) = number {
            node.attributes.insert("number".into(), number);
        }
        for child in &mut node.children {
            if let XMLNode::Text(text) | XMLNode::CData(text) = child {
                *text = text.trim().to_owned();
            }
        }
    });
    Ok(Some(canonical(&value)))
}

fn compatible(measures: &[&Measure]) -> Result<()> {
    let reference = measures[0];
    let mut errors = Vec::new();
    for measure in &measures[1..] {
        if measure.length != reference.length
            || measure.source.attributes.get("number") != reference.source.attributes.get("number")
            || measure
                .source
                .attributes
                .get("implicit")
                .map(String::as_str)
                .unwrap_or("no")
                != reference
                    .source
                    .attributes
                    .get("implicit")
                    .map(String::as_str)
                    .unwrap_or("no")
        {
            errors.push(ScoreError::new(format!(
                "{}, position 0: incompatible measure boundaries with {}",
                measure.context, reference.context
            )));
        }
    }
    for tag in ["time", "key", "transpose"] {
        let mut timelines = Vec::new();
        let mut invalid = false;
        for measure in measures {
            let mut signature = |element: Option<&Element>, position: &Fraction| {
                match state_signature(tag, element) {
                    Ok(value) => value,
                    Err(error) => {
                        errors.push(ScoreError::new(format!(
                            "{}, position {position}: {error}",
                            measure.context
                        )));
                        invalid = true;
                        None
                    }
                }
            };
            let mut timeline = BTreeMap::from([(
                Fraction::zero(),
                signature(measure.state.get(tag), &Fraction::zero()),
            )]);
            for extra in &measure.extras {
                if extra.element.name == "attributes" {
                    if let Some(element) = extra.element.get_child(tag) {
                        timeline.insert(
                            extra.position.clone(),
                            signature(Some(element), &extra.position),
                        );
                    }
                }
            }
            timelines.push(timeline);
        }
        if invalid {
            continue;
        }
        let boundaries: BTreeSet<_> = timelines.iter().flat_map(|t| t.keys().cloned()).collect();
        for position in boundaries {
            let values: Vec<_> = timelines
                .iter()
                .map(|t| t.range(..=position.clone()).next_back().unwrap().1)
                .collect();
            for conflict in (1..values.len()).filter(|index| values[*index] != values[0]) {
                errors.push(ScoreError::new(format!(
                    "{}, position {position}: incompatible {tag} with {}",
                    measures[conflict].context, reference.context
                )));
            }
        }
    }
    check_errors(errors)
}

fn clef(parts: &[&Part]) -> Result<(&'static str, &'static str)> {
    let mut positions = Vec::new();
    let mut errors = Vec::new();
    for part in parts {
        for measure in &part.measures {
            for event in &measure.events {
                for note in &event.notes {
                    if let Some(pitch) = note.get_child("pitch") {
                        let octave = child_text(pitch, "octave")
                            .unwrap_or_default()
                            .parse::<BigInt>();
                        let octave = match octave {
                            Ok(octave) => octave,
                            Err(_) => {
                                errors.push(ScoreError::new(format!(
                                    "{}: Invalid pitch octave",
                                    event.context(measure)
                                )));
                                continue;
                            }
                        };
                        let step = child_text(pitch, "step").unwrap_or_default();
                        let Some(step) = ["C", "D", "E", "F", "G", "A", "B"]
                            .iter()
                            .position(|s| *s == step)
                        else {
                            errors.push(ScoreError::new(format!(
                                "{}: Invalid pitch step",
                                event.context(measure)
                            )));
                            continue;
                        };
                        positions.push(octave * 7 + BigInt::from(step));
                    }
                }
            }
        }
    }
    check_errors(errors)?;
    let burden = |bottom: i32, top: i32| -> BigInt {
        positions
            .iter()
            .map(|p| {
                let below: BigInt = (BigInt::from(bottom) - p) / 2;
                let above: BigInt = (p - BigInt::from(top)) / 2;
                BigInt::zero().max(below).max(above)
            })
            .sum()
    };
    Ok(if burden(30, 38) <= burden(18, 26) {
        ("G", "2")
    } else {
        ("F", "4")
    })
}

fn extras(
    measures: &[&Measure],
    clef: (&str, &str),
    first: bool,
    relationships: &Relationships,
) -> Vec<TimedElement> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for (source, measure) in measures.iter().enumerate() {
        for extra in &measure.extras {
            let mut node = extra.element.clone();
            renumber_relationships(&mut node, source, relationships);
            if node.name == "attributes" {
                if source > 0 {
                    continue;
                }
                remove_children(&mut node, &["clef"]);
                if elements(&node).next().is_none() {
                    continue;
                }
            } else if node.name == "direction" {
                remove_children(&mut node, &["voice"]);
                if !seen.insert((extra.position.clone(), canonical(&node))) {
                    continue;
                }
                let mut voice = Element::new("voice");
                voice.children.push(XMLNode::Text((source + 1).to_string()));
                let index = node
                    .children
                    .iter()
                    .position(|n| {
                        n.as_element().is_some_and(|e| {
                            ["staff", "sound", "listening"].contains(&e.name.as_str())
                        })
                    })
                    .unwrap_or(node.children.len());
                node.children.insert(index, XMLNode::Element(voice));
            } else if !seen.insert((extra.position.clone(), canonical(&node))) {
                continue;
            }
            result.push(TimedElement {
                position: extra.position.clone(),
                element: node,
                divisions: extra.divisions.clone(),
            });
        }
    }
    if first {
        let mut attributes = Element::new("attributes");
        let mut node = Element::new("clef");
        set_child(&mut node, "sign", clef.0);
        set_child(&mut node, "line", clef.1);
        append(&mut attributes, node);
        result.push(TimedElement {
            position: Fraction::zero(),
            element: attributes,
            divisions: measures[0].divisions.clone(),
        });
    }
    result
}

const NUMBERED_RELATIONSHIPS: &[&str] = &[
    "slur",
    "tuplet",
    "glissando",
    "slide",
    "wavy-line",
    "hammer-on",
    "pull-off",
    "wedge",
    "octave-shift",
    "pedal",
    "bracket",
    "dashes",
];
type Relationships = HashMap<(usize, String, String), String>;

fn relationships(parts: &[&Part]) -> Result<Relationships> {
    let mut mapping = HashMap::new();
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let mut errors = Vec::new();
    for (source, part) in parts.iter().enumerate() {
        for measure in &part.measures {
            for node in descendants(&measure.source) {
                if NUMBERED_RELATIONSHIPS.contains(&node.name.as_str()) {
                    let key = (
                        source,
                        node.name.clone(),
                        node.attributes
                            .get("number")
                            .cloned()
                            .unwrap_or_else(|| "1".into()),
                    );
                    if let std::collections::hash_map::Entry::Vacant(entry) = mapping.entry(key) {
                        let count = counts.entry(&node.name).or_default();
                        *count += 1;
                        if *count > 16 {
                            let context = measure
                                .events
                                .iter()
                                .filter(|event| {
                                    event
                                        .notes
                                        .iter()
                                        .any(|note| descendants(note).contains(&node))
                                })
                                .map(|event| event.context(measure))
                                .collect::<Vec<_>>();
                            let context = if context.is_empty() {
                                measure.context.clone()
                            } else {
                                context.join("; ")
                            };
                            errors.push(ScoreError::new(format!(
                                "{context}: too many distinct {} relationships to merge safely",
                                node.name
                            )));
                        }
                        entry.insert(count.to_string());
                    }
                }
            }
        }
    }
    check_errors(errors)?;
    Ok(mapping)
}

fn renumber_relationships(element: &mut Element, source: usize, relationships: &Relationships) {
    walk_mut(element, &mut |node| {
        if NUMBERED_RELATIONSHIPS.contains(&node.name.as_str()) {
            let key = (
                source,
                node.name.clone(),
                node.attributes
                    .get("number")
                    .cloned()
                    .unwrap_or_else(|| "1".into()),
            );
            if let Some(number) = relationships.get(&key) {
                node.attributes.insert("number".into(), number.clone());
            }
        }
    });
}

fn unique_ids(output: &mut Element, definition: &mut Element, used: &mut HashSet<String>) {
    let mut mapping = HashMap::new();
    for root in [&mut *definition, &mut *output] {
        walk_mut(root, &mut |node| {
            if ID_REFERENCES.contains(&node.name.as_str()) {
                return;
            }
            if let Some(identifier) = node.attributes.get("id").cloned() {
                let mut suffix = 1;
                let mut candidate = identifier.clone();
                while used.contains(&candidate) {
                    candidate = format!("{identifier}_merge{suffix}");
                    suffix += 1;
                }
                node.attributes.insert("id".into(), candidate.clone());
                used.insert(candidate.clone());
                mapping.entry(identifier).or_insert(candidate);
            }
        });
    }
    for root in [definition, output] {
        walk_mut(root, &mut |node| {
            if ID_REFERENCES.contains(&node.name.as_str()) {
                if let Some(id) = node
                    .attributes
                    .get("id")
                    .and_then(|id| mapping.get(id))
                    .cloned()
                {
                    node.attributes.insert("id".into(), id);
                }
            }
            for (name, value) in &mut node.attributes {
                if name == "player" {
                    if let Some(renamed) = mapping.get(value) {
                        *value = renamed.clone();
                    }
                } else if name.rsplit(['}', ':']).next() == Some("href") && value.starts_with('#') {
                    if let Some(renamed) = mapping.get(&value[1..]) {
                        *value = format!("#{renamed}");
                    }
                }
            }
        });
    }
}

fn merge_events(
    streams: &[Vec<Event>],
    length: &Fraction,
    measures: &[&Measure],
) -> Result<Vec<Event>> {
    let ordinary: Vec<Vec<_>> = streams
        .iter()
        .map(|s| s.iter().filter(|e| !e.grace()).collect())
        .collect();
    let grace: Vec<Vec<_>> = streams
        .iter()
        .map(|s| s.iter().filter(|e| e.grace()).collect())
        .collect();
    let mut boundaries = BTreeSet::from([Fraction::zero(), length.clone()]);
    for stream in &ordinary {
        for event in stream {
            boundaries.extend([event.start.clone(), event.end()]);
        }
    }
    let boundaries: Vec<_> = boundaries.into_iter().collect();
    let mut divergence = length.clone();
    for pair in boundaries.windows(2) {
        let start = &pair[0];
        let active: Vec<_> = ordinary
            .iter()
            .map(|s| {
                s.iter()
                    .find(|e| e.start <= *start && *start < e.end())
                    .copied()
            })
            .collect();
        let different = if active.iter().any(Option::is_none) {
            true
        } else {
            let reference = active[0].unwrap();
            let signature = signature(reference)?;
            let mut different = false;
            for other in active.iter().skip(1).flatten() {
                if other.start != reference.start
                    || other.duration != reference.duration
                    || self::signature(other)? != signature
                {
                    different = true;
                    break;
                }
            }
            different
        };
        if different {
            divergence = start.clone();
            break;
        }
    }
    let grace_positions: BTreeSet<_> = grace
        .iter()
        .flat_map(|s| s.iter().map(|e| e.start.clone()))
        .collect();
    for position in grace_positions {
        let groups: Vec<Vec<_>> = grace
            .iter()
            .map(|s| {
                s.iter()
                    .filter(|e| e.start == position)
                    .map(|e| signature(e))
                    .collect::<Result<_>>()
            })
            .collect::<Result<_>>()?;
        if groups.iter().skip(1).any(|g| *g != groups[0]) {
            divergence = divergence.min(position);
        }
    }
    let mut result = Vec::new();
    let mut errors = Vec::new();
    for event in ordinary[0].iter().filter(|e| e.start < divergence) {
        let end = event.end().min(divergence.clone());
        let mut merged = match slice_event(event, event.start.clone(), end.clone()) {
            Ok(event) => event,
            Err(error) => {
                errors.push(ScoreError::new(format!(
                    "{}: {error}",
                    source_context(measures[0], event)
                )));
                continue;
            }
        };
        merged.voice = "1".into();
        for stream in ordinary.iter().skip(1) {
            for other in stream
                .iter()
                .filter(|other| other.start < end && other.end() > event.start)
            {
                for note in &mut merged.notes {
                    for other_note in &other.notes {
                        if pitch(note)? == pitch(other_note)? {
                            combine_note(note, other_note);
                        }
                    }
                }
            }
        }
        result.push(merged);
    }
    for (source, stream) in ordinary.iter().enumerate() {
        for event in stream.iter().filter(|e| e.end() > divergence) {
            match slice_event(
                event,
                divergence.clone().max(event.start.clone()),
                event.end(),
            ) {
                Ok(event) => result.push(event),
                Err(error) => errors.push(ScoreError::new(format!(
                    "{}: {error}",
                    source_context(measures[source], event)
                ))),
            }
        }
    }
    for (source, stream) in grace.iter().enumerate() {
        let mut ordinals = HashMap::new();
        for event in stream {
            let ordinal = ordinals.entry(event.start.clone()).or_insert(0_usize);
            let index = *ordinal;
            *ordinal += 1;
            if event.start >= divergence {
                result.push((*event).clone());
            } else if source == 0 {
                let mut merged = (*event).clone();
                for other_stream in grace.iter().skip(1) {
                    let other = other_stream
                        .iter()
                        .filter(|e| e.start == event.start)
                        .nth(index)
                        .unwrap();
                    for (note, other_note) in merged.notes.iter_mut().zip(&other.notes) {
                        combine_note(note, other_note);
                    }
                }
                result.push(merged);
            }
        }
    }
    result.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| b.grace().cmp(&a.grace()))
    });
    check_errors(errors)?;
    deduplicate_notes(result, streams.len())
}

fn deduplicate_notes(events: Vec<Event>, voice_count: usize) -> Result<Vec<Event>> {
    let mut rest_counts = HashMap::<_, usize>::new();
    for event in events.iter().filter(|event| {
        !event.grace()
            && event
                .notes
                .iter()
                .all(|note| note.get_child("rest").is_some())
    }) {
        *rest_counts
            .entry((event.start.clone(), event.duration.clone()))
            .or_default() += 1;
    }
    let mut result: Vec<Event> = Vec::new();
    let mut seen: HashMap<_, (usize, usize)> = HashMap::new();
    let mut grace_counts = HashMap::new();
    for mut event in events {
        let common_rest =
            rest_counts.get(&(event.start.clone(), event.duration.clone())) == Some(&voice_count);
        let ordinal = if event.grace() {
            let count = grace_counts
                .entry((event.start.clone(), event.voice.clone()))
                .or_insert(0_usize);
            let ordinal = *count;
            *count += 1;
            ordinal
        } else {
            0
        };
        for note in std::mem::take(&mut event.notes) {
            let stem = child_text(&note, "stem");
            let rest = note.get_child("rest").is_some();
            if rest && !common_rest || !rest && !matches!(stem.as_deref(), Some("up" | "down")) {
                event.notes.push(note);
                continue;
            }
            let key = (
                event.start.clone(),
                event.duration.clone(),
                event.grace(),
                ordinal,
                pitch(&note)?,
                tie_state(&note),
                stem,
            );
            if let Some(&(event_index, note_index)) = seen.get(&key) {
                let target = if event_index == result.len() {
                    &mut event.notes[note_index]
                } else {
                    &mut result[event_index].notes[note_index]
                };
                combine_note(target, &note);
            } else {
                seen.insert(key, (result.len(), event.notes.len()));
                event.notes.push(note);
            }
        }
        if !event.notes.is_empty() {
            result.push(event);
        }
    }
    Ok(result)
}

fn collect_retained_ids(element: &Element, replaced: &HashSet<&str>, used: &mut HashSet<String>) {
    if ["part", "score-part"].contains(&element.name.as_str())
        && element
            .attributes
            .get("id")
            .is_some_and(|id| replaced.contains(id.as_str()))
    {
        return;
    }
    if let Some(id) = element.attributes.get("id") {
        used.insert(id.clone());
    }
    for child in elements(element) {
        collect_retained_ids(child, replaced, used);
    }
}

/// Merge disjoint groups of stable part IDs; group order controls voice order.
pub fn merge_score(score: &Score, groups: &[Vec<String>]) -> Result<Element> {
    let stems: Vec<Vec<String>> = groups
        .iter()
        .map(|group| {
            (0..group.len())
                .map(|index| if index % 2 == 0 { "up" } else { "down" }.into())
                .collect()
        })
        .collect();
    merge_score_with_stems(score, groups, &stems)
}

/// Merge with one explicit up/down stem direction per source, in group voice order.
pub fn merge_score_with_stems(
    score: &Score,
    groups: &[Vec<String>],
    stems: &[Vec<String>],
) -> Result<Element> {
    if stems.len() != groups.len()
        || groups.iter().zip(stems).any(|(group, stems)| {
            group.len() != stems.len()
                || stems
                    .iter()
                    .any(|stem| !["up", "down"].contains(&stem.as_str()))
        })
    {
        return Err(ScoreError::new(
            "Each merge source requires an up or down stem direction",
        ));
    }
    let groups = select_groups(score, groups)?;
    let replaced: HashSet<_> = groups
        .iter()
        .flatten()
        .map(|i| score.parts[*i].id())
        .collect();
    let mut used_ids = HashSet::new();
    collect_retained_ids(&score.root, &replaced, &mut used_ids);
    let mut replacements = HashMap::new();
    let mut errors = Vec::new();
    for (indices, stems) in groups.into_iter().zip(stems) {
        let parts: Vec<_> = indices.iter().map(|i| &score.parts[*i]).collect();
        if parts
            .iter()
            .any(|p| p.measures.len() != parts[0].measures.len())
        {
            errors.push(ScoreError::new(format!(
                "Cannot merge parts with different numbers of measures: {}",
                parts
                    .iter()
                    .map(|p| p.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        let anchor = &score.parts[*indices.iter().min().unwrap()];
        let mut definition = anchor.definition.clone();
        set_child(
            &mut definition,
            "part-name",
            parts
                .iter()
                .map(|p| p.name())
                .collect::<Vec<_>>()
                .join(" / "),
        );
        remove_children(
            &mut definition,
            &[
                "part-name-display",
                "part-abbreviation",
                "part-abbreviation-display",
            ],
        );
        for tag in [
            "score-instrument",
            "player",
            "midi-device",
            "midi-instrument",
        ] {
            remove_children(&mut definition, &[tag]);
            for part in &parts {
                for item in elements(&part.definition).filter(|e| e.name == tag) {
                    append(&mut definition, item.clone());
                }
            }
        }
        let mut output = Element::new("part");
        output.attributes = anchor.source.attributes.clone();
        let lines = lyric_lines(&parts);
        let relationships = match relationships(&parts) {
            Ok(relationships) => Some(relationships),
            Err(error) => {
                errors.push(error);
                None
            }
        };
        let clef = match clef(&parts) {
            Ok(clef) => Some(clef),
            Err(error) => {
                errors.push(error);
                None
            }
        };
        for index in 0..parts.iter().map(|part| part.measures.len()).max().unwrap() {
            let measures: Vec<_> = parts.iter().filter_map(|p| p.measures.get(index)).collect();
            let mut failed = measures.len() != parts.len();
            if let Err(error) = compatible(&measures) {
                errors.push(error);
                failed = true;
            }
            let mut streams = Vec::new();
            for (source, measure) in measures.iter().enumerate() {
                match flatten(measure, &(source + 1).to_string()) {
                    Ok(stream) => streams.push(stream),
                    Err(error) => {
                        errors.push(error);
                        failed = true;
                    }
                }
            }
            if failed {
                continue;
            }
            let (Some(relationships), Some(clef)) = (&relationships, clef) else {
                continue;
            };
            for (source, stream) in streams.iter_mut().enumerate() {
                let instrument = parts[source]
                    .definition
                    .get_child("score-instrument")
                    .and_then(|i| i.attributes.get("id"))
                    .map(String::as_str);
                prepare(
                    stream,
                    source,
                    &stems[source],
                    &lines,
                    relationships,
                    instrument,
                );
            }
            let events = match merge_events(&streams, &measures[0].length, &measures) {
                Ok(events) => events,
                Err(error) => {
                    errors.push(error);
                    continue;
                }
            };
            let extras = extras(&measures, clef, index == 0, relationships);
            match render_measure(measures[0], &events, Some(&extras), None, None) {
                Ok(rendered) => append(&mut output, rendered),
                Err(error) => errors.push(error),
            }
        }
        unique_ids(&mut output, &mut definition, &mut used_ids);
        for part in parts {
            replacements.insert(part.id().to_owned(), Vec::new());
        }
        replacements.insert(anchor.id().to_owned(), vec![(definition, output)]);
    }
    check_errors(errors)?;
    Ok(build_score(score, &replacements))
}
