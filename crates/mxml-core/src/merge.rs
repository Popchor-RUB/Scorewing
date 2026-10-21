//! Merge selected single-staff parts into ordered, stem-directed voices.

use crate::model::{
    build_score, insert_note_child, render_measure, rest_event, set_child, slice_event, Event,
    Fraction, Measure, Part, Score, TimedElement,
};
use crate::split::{
    append, child_text, descendants, elements, elements_mut, numeric, remove_children, voice_cmp,
    walk_mut, ID_REFERENCES,
};
use crate::{Result, ScoreError};
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

fn flatten(measure: &Measure, voice: &str) -> Result<Vec<Event>> {
    let mut by_span = Vec::<Event>::new();
    let mut indices = HashMap::new();
    let mut grace_counts = HashMap::new();
    for event in &measure.events {
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
            return Err(ScoreError::new(format!("{}, position {}: incompatible overlapping rhythmic spans cannot be flattened into one voice", measure.context, event.start)));
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
        end = event.end();
        completed.push(event);
    }
    if end < measure.length {
        completed.push(rest_event(end.clone(), &measure.length - &end, voice)?);
    }
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
                let odd = event.voice.parse::<usize>().unwrap_or(1) % 2 == 1;
                set_child(note, "stem", if odd { "up" } else { "down" });
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
            return Err(ScoreError::new(format!(
                "{}, position 0: incompatible measure boundaries with {}",
                measure.context, reference.context
            )));
        }
    }
    for tag in ["time", "key", "transpose"] {
        let mut timelines = Vec::new();
        for measure in measures {
            let mut timeline = BTreeMap::from([(
                Fraction::zero(),
                state_signature(tag, measure.state.get(tag))?,
            )]);
            for extra in &measure.extras {
                if extra.element.name == "attributes" {
                    if let Some(element) = extra.element.get_child(tag) {
                        timeline
                            .insert(extra.position.clone(), state_signature(tag, Some(element))?);
                    }
                }
            }
            timelines.push(timeline);
        }
        let boundaries: BTreeSet<_> = timelines.iter().flat_map(|t| t.keys().cloned()).collect();
        for position in boundaries {
            let values: Vec<_> = timelines
                .iter()
                .map(|t| t.range(..=position.clone()).next_back().unwrap().1)
                .collect();
            if let Some(conflict) = values.iter().position(|value| *value != values[0]) {
                return Err(ScoreError::new(format!(
                    "{}, position {position}: incompatible {tag} with {}",
                    measures[conflict].context, reference.context
                )));
            }
        }
    }
    Ok(())
}

fn clef(parts: &[&Part]) -> Result<(&'static str, &'static str)> {
    let mut positions = Vec::new();
    for part in parts {
        for measure in &part.measures {
            for event in &measure.events {
                for note in &event.notes {
                    if let Some(pitch) = note.get_child("pitch") {
                        let octave: BigInt = child_text(pitch, "octave")
                            .unwrap_or_default()
                            .parse()
                            .map_err(|_| ScoreError::new("Invalid pitch octave"))?;
                        let step = child_text(pitch, "step").unwrap_or_default();
                        let step = ["C", "D", "E", "F", "G", "A", "B"]
                            .iter()
                            .position(|s| *s == step)
                            .ok_or_else(|| ScoreError::new("Invalid pitch step"))?;
                        positions.push(octave * 7 + BigInt::from(step));
                    }
                }
            }
        }
    }
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
    for (source, part) in parts.iter().enumerate() {
        for node in descendants(&part.source) {
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
                        return Err(ScoreError::new(format!(
                            "Part {:?}: too many distinct {} relationships to merge safely",
                            part.name(),
                            node.name
                        )));
                    }
                    entry.insert(count.to_string());
                }
            }
        }
    }
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

fn merge_events(streams: &[Vec<Event>], length: &Fraction) -> Result<Vec<Event>> {
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
    for event in ordinary[0].iter().filter(|e| e.start < divergence) {
        let end = event.end().min(divergence.clone());
        let mut merged = slice_event(event, event.start.clone(), end.clone())?;
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
    for stream in &ordinary {
        for event in stream.iter().filter(|e| e.end() > divergence) {
            result.push(slice_event(
                event,
                divergence.clone().max(event.start.clone()),
                event.end(),
            )?);
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
    let groups = select_groups(score, groups)?;
    let replaced: HashSet<_> = groups
        .iter()
        .flatten()
        .map(|i| score.parts[*i].id())
        .collect();
    let mut used_ids = HashSet::new();
    collect_retained_ids(&score.root, &replaced, &mut used_ids);
    let mut replacements = HashMap::new();
    for indices in groups {
        let parts: Vec<_> = indices.iter().map(|i| &score.parts[*i]).collect();
        if parts
            .iter()
            .any(|p| p.measures.len() != parts[0].measures.len())
        {
            return Err(ScoreError::new(format!(
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
        let relationships = relationships(&parts)?;
        let clef = clef(&parts)?;
        for index in 0..parts[0].measures.len() {
            let measures: Vec<_> = parts.iter().map(|p| &p.measures[index]).collect();
            compatible(&measures)?;
            let mut streams: Vec<_> = measures
                .iter()
                .enumerate()
                .map(|(source, measure)| flatten(measure, &(source + 1).to_string()))
                .collect::<Result<_>>()?;
            for (source, stream) in streams.iter_mut().enumerate() {
                let instrument = parts[source]
                    .definition
                    .get_child("score-instrument")
                    .and_then(|i| i.attributes.get("id"))
                    .map(String::as_str);
                prepare(stream, source, &lines, &relationships, instrument);
            }
            let events = merge_events(&streams, &measures[0].length)?;
            let extras = extras(&measures, clef, index == 0, &relationships);
            append(
                &mut output,
                render_measure(measures[0], &events, Some(&extras), None, None)?,
            );
        }
        unique_ids(&mut output, &mut definition, &mut used_ids);
        for part in parts {
            replacements.insert(part.id().to_owned(), Vec::new());
        }
        replacements.insert(anchor.id().to_owned(), vec![(definition, output)]);
    }
    Ok(build_score(score, &replacements))
}
