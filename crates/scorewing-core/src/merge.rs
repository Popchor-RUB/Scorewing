//! Merge selected single-staff parts into ordered, stem-directed voices.

use crate::model::{
    build_score, insert_note_child, render_measure, set_child, set_written_duration, Event,
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

mod dynamics;

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

fn source_events(measure: &Measure) -> Result<Vec<Event>> {
    let mut errors = Vec::new();
    for event in &measure.events {
        for note in &event.notes {
            let validation = pitch(note).and_then(|_| {
                if note.get_child("time-modification").is_some() {
                    set_written_duration(&mut note.clone(), event.duration.clone())?;
                }
                Ok(())
            });
            if let Err(error) = validation {
                errors.push(ScoreError::new(format!(
                    "{}: {error}",
                    event.context(measure)
                )));
            }
        }
    }
    check_errors(errors)?;
    Ok(measure.events.clone())
}

fn lyric_identity(lyric: &Element) -> String {
    let mut identity = lyric.clone();
    // Verse and sung content matter; engraving coordinates and typography do not.
    walk_mut(&mut identity, &mut |node| {
        node.attributes.retain(|key, _| {
            if node.name == "lyric" {
                ["number", "name", "time-only"].contains(&key.as_str())
            } else {
                ["type", "xml:lang", "lang"].contains(&key.as_str())
            }
        });
        node.children.retain(|child| {
            !matches!(
                child,
                XMLNode::Comment(_) | XMLNode::ProcessingInstruction(..)
            )
        });
    });
    identity
        .attributes
        .entry("number".into())
        .or_insert_with(|| "1".into());
    let mut content: Vec<XMLNode> = Vec::new();
    for node in elements(&identity) {
        if ["end-line", "end-paragraph"].contains(&node.name.as_str())
            || node.name == "syllabic" && node.get_text().as_deref() == Some("single")
        {
            continue;
        }
        if let Some(previous) = content.last_mut().and_then(XMLNode::as_mut_element) {
            if node.name == "text"
                && previous.name == "text"
                && node.attributes == previous.attributes
            {
                previous.children.extend(node.children.clone());
                continue;
            }
        }
        content.push(XMLNode::Element(node.clone()));
    }
    identity.children = content;
    canonical(&identity)
}

fn deduplicate_lyrics(streams: &mut [Vec<Event>], stems: &[String]) {
    let mut seen = HashSet::new();
    let mut sources: Vec<_> = (0..streams.len()).collect();
    sources.sort_by_key(|source| stems[*source] != "up");
    for source in sources {
        let mut grace_counts = HashMap::new();
        for event in &mut streams[source] {
            let grace = event.grace();
            let ordinal = if grace {
                let count = grace_counts
                    .entry((event.start.clone(), event.voice.clone()))
                    .or_insert(0);
                let ordinal = *count;
                *count += 1;
                ordinal
            } else {
                0
            };
            for note in &mut event.notes {
                note.children.retain(|node| {
                    node.as_element().is_none_or(|lyric| {
                        lyric.name != "lyric"
                            || seen.insert((
                                event.start.clone(),
                                grace,
                                ordinal,
                                lyric_identity(lyric),
                            ))
                    })
                });
            }
        }
    }
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

fn lyric_lines(parts: &[&Part], stems: &[String]) -> LyricLines {
    let mut mapping = HashMap::new();
    let mut sources: Vec<_> = (0..parts.len()).collect();
    sources.sort_by_key(|source| stems[*source] != "up");
    for source in sources {
        let part = parts[source];
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
                lyric.attributes.insert("placement".into(), "below".into());
                for attribute in ["default-y", "relative-y"] {
                    lyric.attributes.remove(attribute);
                }
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
    stems: &[String],
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
                voice.children.push(XMLNode::Text(
                    if stems[source] == "up" { "1" } else { "2" }.into(),
                ));
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
            for node in measure
                .events
                .iter()
                .flat_map(|event| &event.notes)
                .chain(measure.extras.iter().map(|extra| &extra.element))
                .flat_map(descendants)
            {
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

#[derive(Default)]
struct VoiceSlots {
    extra: HashMap<(usize, usize), String>,
    ties: HashMap<(usize, String, Pitch), usize>,
}

impl VoiceSlots {
    fn voice(&mut self, group: usize, lane: usize) -> String {
        if lane == 0 {
            return (group + 1).to_string();
        }
        let next = (self.extra.len() + 3).to_string();
        self.extra.entry((group, lane)).or_insert(next).clone()
    }
}

fn rhythm(event: &Event) -> Vec<String> {
    elements(&event.notes[0])
        .filter(|node| ["type", "dot", "time-modification", "grace"].contains(&node.name.as_str()))
        .map(|node| {
            let mut node = node.clone();
            walk_mut(&mut node, &mut |node| node.attributes.clear());
            canonical(&node)
        })
        .collect()
}

fn combine_events(target: &mut Event, other: &Event) -> Result<()> {
    for note in &other.notes {
        let key = (pitch(note)?, tie_state(note));
        if let Some(own) = target
            .notes
            .iter_mut()
            .find(|own| pitch(own).ok().as_ref() == Some(&key.0) && tie_state(own) == key.1)
        {
            combine_note(own, note);
        } else {
            target.notes.push(note.clone());
        }
    }
    Ok(())
}

fn has_tie(note: &Element, kind: &str) -> bool {
    let (sound, visual) = tie_state(note);
    sound
        .iter()
        .chain(&visual)
        .any(|value| value == kind || value == "continue")
}

fn merge_events(
    streams: &[Vec<Event>],
    stems: &[String],
    slots: &mut VoiceSlots,
) -> Result<Vec<Event>> {
    let mut result = Vec::new();
    let mut voice_groups = HashMap::new();
    for (group, stem) in ["up", "down"].into_iter().enumerate() {
        let mut spans: Vec<(Vec<(usize, &Event)>, usize)> = Vec::new();
        let mut indices = HashMap::new();
        for (source, stream) in streams
            .iter()
            .enumerate()
            .filter(|(source, _)| stems[*source] == stem)
        {
            let mut grace_counts = HashMap::new();
            for event in stream {
                let ordinal = if event.grace() {
                    let count = grace_counts
                        .entry((event.start.clone(), event.voice.clone()))
                        .or_insert(0);
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
                    rhythm(event),
                    event.notes[0].get_child("rest").is_some(),
                );
                if let Some(index) = indices.get(&key) {
                    let index: usize = *index;
                    spans[index].0.push((source, event));
                } else {
                    indices.insert(key, spans.len());
                    spans.push((vec![(source, event)], ordinal));
                }
            }
        }
        spans.sort_by(|(a, ai), (b, bi)| {
            let (a, b) = (a[0].1, b[0].1);
            a.start
                .cmp(&b.start)
                .then_with(|| b.grace().cmp(&a.grace()))
                .then(ai.cmp(bi))
                .then_with(|| b.duration.cmp(&a.duration))
        });
        let mut ends: Vec<Fraction> = Vec::new();
        for (owners, _) in spans {
            let mut partitions: BTreeMap<Option<usize>, Vec<(usize, &Event)>> = BTreeMap::new();
            for (source, event) in owners {
                let mut required = BTreeSet::new();
                for note in &event.notes {
                    if has_tie(note, "stop") {
                        if let Some(lane) =
                            slots.ties.get(&(source, event.voice.clone(), pitch(note)?))
                        {
                            required.insert(*lane);
                        }
                    }
                }
                partitions
                    .entry(required.first().copied())
                    .or_default()
                    .push((source, event));
            }
            if partitions.len() > 1 {
                if let Some(unbound) = partitions.remove(&None) {
                    partitions.first_entry().unwrap().get_mut().extend(unbound);
                }
            }
            for (required, owners) in partitions {
                let mut event = owners[0].1.clone();
                for (_, other) in owners.iter().skip(1) {
                    combine_events(&mut event, other)?;
                }
                let lane = if event.grace() {
                    required.unwrap_or(0)
                } else {
                    // A previously shared tied chord may now require independent rhythms.
                    required
                        .filter(|lane| ends.get(*lane).is_none_or(|end| *end <= event.start))
                        .unwrap_or_else(|| {
                            ends.iter()
                                .position(|end| *end <= event.start)
                                .unwrap_or(ends.len())
                        })
                };
                if !event.grace() {
                    ends.resize_with(ends.len().max(lane + 1), Fraction::zero);
                    ends[lane] = event.end();
                }
                for (source, original) in owners {
                    for note in &original.notes {
                        let key = (source, original.voice.clone(), pitch(note)?);
                        if has_tie(note, "stop") {
                            slots.ties.remove(&key);
                        }
                        if has_tie(note, "start") {
                            slots.ties.insert(key, lane);
                        }
                    }
                }
                event.voice = slots.voice(group, lane);
                voice_groups.insert(event.voice.clone(), group);
                result.push(event);
            }
        }
    }
    result.sort_by(|a, b| voice_cmp(&a.voice, &b.voice).then(a.start.cmp(&b.start)));
    let mut rests: HashMap<_, usize> = HashMap::new();
    let mut deduplicated: Vec<Event> = Vec::new();
    let mut removed_rest = false;
    for event in result {
        if !event.grace()
            && event
                .notes
                .iter()
                .all(|note| note.get_child("rest").is_some())
        {
            let key = (event.start.clone(), event.duration.clone());
            if let Some(index) = rests.get(&key) {
                combine_note(&mut deduplicated[*index].notes[0], &event.notes[0]);
                removed_rest = true;
                continue;
            }
            rests.insert(key, deduplicated.len());
        }
        deduplicated.push(event);
    }
    if removed_rest {
        compact_after_rests(&mut deduplicated, &voice_groups, slots);
    }
    Ok(deduplicated)
}

fn compact_after_rests(
    events: &mut [Event],
    groups: &HashMap<String, usize>,
    slots: &mut VoiceSlots,
) {
    let connected = |event: &Event| {
        event
            .notes
            .iter()
            .any(|note| has_tie(note, "start") || has_tie(note, "stop"))
    };
    let fixed: Vec<_> = events
        .iter()
        .filter(|event| !event.grace() && connected(event))
        .map(|event| (event.voice.clone(), event.start.clone(), event.end()))
        .collect();
    events.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| b.grace().cmp(&a.grace()))
            .then_with(|| b.duration.cmp(&a.duration))
    });
    let mut ends: HashMap<String, Fraction> = HashMap::new();
    for event in events.iter_mut().filter(|event| !event.grace()) {
        if !connected(event) {
            let group = groups[&event.voice];
            let mut lane = 0;
            loop {
                let voice = slots.voice(group, lane);
                if ends.get(&voice).is_none_or(|end| *end <= event.start)
                    && !fixed.iter().any(|(fixed_voice, start, end)| {
                        *fixed_voice == voice && *start < event.end() && *end > event.start
                    })
                {
                    event.voice = voice;
                    break;
                }
                lane += 1;
            }
        }
        ends.insert(event.voice.clone(), event.end());
    }
    events.sort_by(|a, b| voice_cmp(&a.voice, &b.voice).then(a.start.cmp(&b.start)));
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

/// Merge disjoint groups of stable part IDs, with alternating up/down source stems.
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

/// Merge sources into as few rhythm-compatible voices as possible per stem group.
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
        let mut owned_parts: Vec<_> = indices.iter().map(|i| score.parts[*i].clone()).collect();
        if let Err(error) = dynamics::normalize(&mut owned_parts, stems) {
            errors.push(error);
            continue;
        }
        let parts: Vec<_> = owned_parts.iter().collect();
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
        let lines = lyric_lines(&parts, stems);
        let mut slots = VoiceSlots::default();
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
            for measure in &measures {
                match source_events(measure) {
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
            deduplicate_lyrics(&mut streams, stems);
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
            let events = match merge_events(&streams, stems, &mut slots) {
                Ok(events) => events,
                Err(error) => {
                    errors.push(error);
                    continue;
                }
            };
            let extras = extras(&measures, stems, clef, index == 0, relationships);
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
