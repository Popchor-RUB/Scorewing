//! Separate explicit voices and chord tones without guessing melodic lines.

pub(crate) use crate::model::compare_voices as voice_cmp;
use crate::model::{
    build_score, insert_note_child, render_measure, rest_event, set_child, slice_event,
    strip_position, Event, Fraction, Measure, Part, Score, TimedElement,
};
pub(crate) use crate::xml::{append, child_text, elements, visit_mut as walk_mut};
use crate::{check_errors, Result, ScoreError};
use num_bigint::BigInt;
use num_traits::Zero;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use xmltree::{Element, XMLNode};

/// A generated staff identified by its primary voice and chord tone, with a suggested fallback.
/// Compatible native voices may share extra chord-tone staffs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitLane {
    pub voice: String,
    /// Zero-based chord-tone position, highest pitch first.
    pub tone: usize,
    pub fallback_voice: Option<String>,
}

/// An explicit fallback choice for one generated staff.
#[derive(Debug, Clone)]
pub struct SplitFallback {
    pub part_id: String,
    pub voice: String,
    pub tone: usize,
    pub fallback_voice: Option<String>,
}

fn split_lanes(part: &Part, mode: &str) -> Result<Vec<SplitLane>> {
    if !["chords", "voices"].contains(&mode) {
        return Err(ScoreError::new("Split mode must be 'chords' or 'voices'"));
    }
    let mut widths: HashMap<&str, usize> = HashMap::new();
    for event in part.measures.iter().flat_map(|measure| &measure.events) {
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
    let mut voices: Vec<_> = widths.keys().copied().collect();
    voices.sort_by(|a, b| voice_cmp(a, b));
    let lanes: Vec<_> = voices
        .iter()
        .flat_map(|voice| {
            (0..widths[voice]).map(move |tone| SplitLane {
                voice: (*voice).to_owned(),
                tone,
                fallback_voice: None,
            })
        })
        .collect();
    Ok(if lanes.len() < 2 { Vec::new() } else { lanes })
}

struct AllocatedLane {
    slot: SplitLane,
    shared_voices: Vec<String>,
    required: Vec<bool>,
}

impl AllocatedLane {
    fn native(&self, voice: &str, measure: usize) -> bool {
        self.shared_voices.iter().any(|source| source == voice)
            || (self.slot.voice == voice
                && (self.shared_voices.is_empty() || self.required[measure]))
    }
}

fn voice_present(measure: &Measure, voice: &str) -> bool {
    measure.events.iter().any(|event| event.voice == voice)
        || measure
            .extras
            .iter()
            .any(|extra| child_text(&extra.element, "voice").as_deref() == Some(voice))
}

fn voice_layout(part: &Part, voice: &str) -> Option<(String, String)> {
    let mut staffs = HashSet::new();
    let mut stems = HashSet::new();
    for note in part
        .measures
        .iter()
        .flat_map(|measure| &measure.events)
        .filter(|event| event.voice == voice)
        .flat_map(|event| &event.notes)
        .filter(|note| note.get_child("rest").is_none())
    {
        staffs.insert(child_text(note, "staff").unwrap_or_else(|| "1".into()));
        if let Some(stem) = child_text(note, "stem").filter(|s| s == "up" || s == "down") {
            stems.insert(stem);
        }
    }
    if staffs.len() != 1 || stems.len() != 1 {
        return None;
    }
    Some((staffs.into_iter().next()?, stems.into_iter().next()?))
}

fn required_measures(part: &Part, slot: &SplitLane) -> Vec<bool> {
    let mut required = vec![false; part.measures.len()];
    let mut ties = HashMap::new();
    let mut slurs = HashMap::new();
    let mut spans = Vec::new();
    for (index, measure) in part.measures.iter().enumerate() {
        for event in measure
            .events
            .iter()
            .filter(|event| event.voice == slot.voice)
        {
            required[index] |= event.notes.len() > slot.tone;
            let Ok(selected) = notes(event, slot.tone, "chords", measure) else {
                return vec![true; part.measures.len()];
            };
            for note in selected {
                let Ok(key) = pitch(note) else {
                    return vec![true; part.measures.len()];
                };
                if has_tie(note, "stop") {
                    if let Some(start) = ties.remove(&key) {
                        spans.push((start, index));
                    }
                }
                if has_tie(note, "start") {
                    ties.entry(key).or_insert(index);
                }
                if let Some(notations) = note.get_child("notations") {
                    for slur in elements(notations).filter(|node| node.name == "slur") {
                        let number = slur
                            .attributes
                            .get("number")
                            .map(String::as_str)
                            .unwrap_or("1")
                            .to_owned();
                        match slur.attributes.get("type").map(String::as_str) {
                            Some("start") => {
                                slurs.entry(number).or_insert(index);
                            }
                            Some("stop") => {
                                if let Some(start) = slurs.remove(&number) {
                                    spans.push((start, index));
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    spans.extend(
        ties.into_values()
            .chain(slurs.into_values())
            .map(|start| (start, part.measures.len() - 1)),
    );
    // A relationship touching a real chord tone also reserves its single-note continuations.
    loop {
        let mut changed = false;
        for &(start, end) in &spans {
            if required[start..=end].iter().any(|needed| *needed) {
                for needed in &mut required[start..=end] {
                    changed |= !*needed;
                    *needed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    required
}

fn allocation_plan(part: &Part, mode: &str) -> Result<Vec<AllocatedLane>> {
    let slots = split_lanes(part, mode)?;
    let mut lanes: Vec<_> = slots
        .iter()
        .map(|slot| AllocatedLane {
            slot: slot.clone(),
            shared_voices: Vec::new(),
            required: if mode == "chords" && slot.tone > 0 {
                required_measures(part, slot)
            } else {
                vec![true; part.measures.len()]
            },
        })
        .collect();
    if mode != "chords" {
        return Ok(lanes);
    }
    let mut absorbed = HashSet::new();
    for source in slots.iter().filter(|slot| slot.tone == 0) {
        if slots
            .iter()
            .any(|slot| slot.voice == source.voice && slot.tone > 0)
        {
            continue;
        }
        let Some(layout) = voice_layout(part, &source.voice) else {
            continue;
        };
        let candidates: Vec<_> = lanes
            .iter()
            .enumerate()
            .filter(|(_, target)| {
                target.slot.tone > 0
                    && voice_cmp(&target.slot.voice, &source.voice).is_lt()
                    && voice_layout(part, &target.slot.voice).as_ref() == Some(&layout)
                    && part.measures.iter().enumerate().all(|(index, measure)| {
                        !voice_present(measure, &source.voice)
                            || (!target.required[index]
                                && !target
                                    .shared_voices
                                    .iter()
                                    .any(|voice| voice_present(measure, voice)))
                    })
            })
            .map(|(index, _)| index)
            .collect();
        if let [index] = candidates.as_slice() {
            lanes[*index].shared_voices.push(source.voice.clone());
            absorbed.insert(source.voice.as_str());
        }
    }
    lanes.retain(|lane| !absorbed.contains(lane.slot.voice.as_str()));
    Ok(lanes)
}

/// Preview the exact generated staff order and conservative, stem-based fallbacks.
pub fn split_plan(part: &Part, mode: &str) -> Result<Vec<SplitLane>> {
    let mut lanes: Vec<_> = allocation_plan(part, mode)?
        .into_iter()
        .map(|lane| lane.slot)
        .collect();
    let mut stems: HashMap<&str, HashSet<String>> = HashMap::new();
    let present: Vec<HashSet<&str>> = part
        .measures
        .iter()
        .map(|measure| {
            measure
                .events
                .iter()
                .map(|event| event.voice.as_str())
                .collect()
        })
        .collect();
    for event in part.measures.iter().flat_map(|measure| &measure.events) {
        for note in &event.notes {
            if note.get_child("rest").is_none() {
                if let Some(stem) =
                    child_text(note, "stem").filter(|stem| stem == "up" || stem == "down")
                {
                    stems.entry(&event.voice).or_default().insert(stem);
                }
            }
        }
    }
    for lane in &mut lanes {
        let Some(stem) = stems
            .get(lane.voice.as_str())
            .filter(|stems| stems.len() == 1)
        else {
            continue;
        };
        let mut best = 0;
        let mut tied = false;
        for (candidate, candidate_stem) in &stems {
            if *candidate == lane.voice || candidate_stem != stem {
                continue;
            }
            let count = present
                .iter()
                .filter(|voices| {
                    !voices.contains(lane.voice.as_str()) && voices.contains(candidate)
                })
                .count();
            if count > best {
                best = count;
                tied = false;
                lane.fallback_voice = Some((*candidate).to_owned());
            } else if count == best && count > 0 {
                tied = true;
            }
        }
        if tied {
            lane.fallback_voice = None;
        }
    }
    Ok(lanes)
}

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

fn notes<'a>(
    event: &'a Event,
    tone: usize,
    mode: &str,
    measure: &Measure,
) -> Result<Vec<&'a Element>> {
    if mode == "voices" || event.notes.len() == 1 {
        return Ok(event.notes.iter().collect());
    }
    let mut keyed = Vec::new();
    let mut errors = Vec::new();
    for note in &event.notes {
        match pitch(note) {
            Ok(Some(pitch)) => keyed.push((pitch, note)),
            Ok(None) => errors.push(ScoreError::new(format!(
                "{}: a chord cannot contain rests",
                event.context(measure)
            ))),
            Err(error) => errors.push(ScoreError::new(format!(
                "{}: {error}",
                event.context(measure)
            ))),
        }
    }
    check_errors(errors)?;
    keyed.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(keyed
        .get(tone)
        .map(|(_, note)| vec![*note])
        .unwrap_or_default())
}

fn segments(
    measure: &Measure,
    voice: Option<&str>,
    shared: bool,
) -> Vec<(Fraction, Fraction, Option<usize>)> {
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
        let event = voice.and_then(|voice| {
            active
                .get(voice)
                .copied()
                .or_else(|| (shared && active.len() == 1).then(|| *active.values().next().unwrap()))
        });
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
    allocation: &AllocatedLane,
    mode: &str,
    assignments: &mut Assignments,
    lane: usize,
    configured: bool,
) -> Result<(Vec<Event>, Vec<TimedElement>)> {
    let slot = &allocation.slot;
    let shared_voice = allocation
        .shared_voices
        .iter()
        .find(|voice| voice_present(measure, voice));
    let shared = !configured && shared_voice.is_none();
    let tone = if shared_voice.is_some() { 0 } else { slot.tone };
    let voice = if let Some(voice) = shared_voice {
        Some(voice.as_str())
    } else if measure.events.iter().any(|event| event.voice == slot.voice) {
        Some(slot.voice.as_str())
    } else if configured {
        slot.fallback_voice
            .as_deref()
            .filter(|voice| measure.events.iter().any(|event| event.voice == *voice))
    } else {
        Some("1")
    };
    let mut result = Vec::new();
    let mut errors = Vec::new();
    for (start, end, index) in segments(measure, voice, shared) {
        let source = index.map(|index| &measure.events[index]);
        let selected_notes = match source {
            Some(event) => match notes(event, tone, mode, measure) {
                Ok(notes) => notes,
                Err(error) => {
                    errors.push(error);
                    continue;
                }
            },
            None => Vec::new(),
        };
        if selected_notes.is_empty() {
            result.push(rest_event(start.clone(), &end - &start, "1")?);
            continue;
        }
        let source = source.unwrap();
        for original in &selected_notes {
            assign(assignments, original, lane);
        }
        // Retain provenance until connections at shared-voice transitions are repaired.
        let selected = Event {
            start: source.start.clone(),
            duration: source.duration.clone(),
            notes: selected_notes.iter().map(|n| (*n).clone()).collect(),
            voice: source.voice.clone(),
        };
        let sliced = start != source.start || end != source.end();
        let mut event = if sliced {
            match slice_event(&selected, start.clone(), end.clone()) {
                Ok(event) => event,
                Err(error) => {
                    errors.push(ScoreError::new(format!(
                        "{}: {error}",
                        source.context(measure)
                    )));
                    continue;
                }
            }
        } else {
            selected
        };
        for (original, copied) in selected_notes.iter().zip(&mut event.notes) {
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
        if Some(source.voice.as_str()) != voice && (!shared || occupied.len() != 1) {
            continue;
        }
        let selected = match notes(source, tone, mode, measure) {
            Ok(notes) => notes,
            Err(error) => {
                errors.push(error);
                continue;
            }
        };
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
            voice: source.voice.clone(),
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
            if Some(source_voice.as_str()) != voice
                && !(shared && occupied.len() == 1 && occupied.contains(source_voice.as_str()))
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
    check_errors(errors)?;
    Ok((result, extras))
}

fn has_tie(note: &Element, kind: &str) -> bool {
    elements(note)
        .filter(|e| e.name == "tie")
        .chain(
            note.get_child("notations")
                .into_iter()
                .flat_map(elements)
                .filter(|e| e.name == "tied"),
        )
        .any(|e| e.attributes.get("type").is_some_and(|value| value == kind))
}

fn set_tie(note: &mut Element, kind: &str, enabled: bool) {
    for tag in ["tie", "tied"] {
        if enabled && tag == "tied" && note.get_child("notations").is_none() {
            insert_note_child(note, Element::new("notations"));
        }
        let parent = if tag == "tie" {
            Some(&mut *note)
        } else {
            note.get_mut_child("notations")
        };
        let Some(parent) = parent else { continue };
        let matches = |e: &Element| {
            e.name == tag && e.attributes.get("type").is_some_and(|value| value == kind)
        };
        if !enabled {
            parent
                .children
                .retain(|node| node.as_element().is_none_or(|e| !matches(e)));
        } else if !elements(parent).any(matches) {
            let mut mark = Element::new(tag);
            mark.attributes.insert("type".into(), kind.into());
            if tag == "tie" {
                insert_note_child(parent, mark);
            } else {
                append(parent, mark);
            }
        }
    }
}

fn connect_shared_voices(
    part: &Part,
    measures: &mut [(Vec<Event>, Vec<TimedElement>)],
    configured: bool,
) -> Result<Option<String>> {
    let used_slurs: HashSet<_> = measures
        .iter()
        .flat_map(|(events, _)| events)
        .flat_map(|event| &event.notes)
        .flat_map(descendants)
        .filter(|e| e.name == "slur")
        .map(|e| {
            e.attributes
                .get("number")
                .cloned()
                .unwrap_or_else(|| "1".into())
        })
        .collect();
    let slur_number = (1..=16)
        .map(|n| n.to_string())
        .find(|n| !used_slurs.contains(n));
    let mut errors = Vec::new();
    let mut previous: Option<(usize, &mut Event)> = None;
    for (index, event) in measures
        .iter_mut()
        .enumerate()
        .flat_map(|(index, (events, _))| events.iter_mut().map(move |event| (index, event)))
        .filter(|(_, event)| !event.grace())
    {
        if let Some((previous_index, left)) = previous {
            let checked = (|| -> Result<()> {
                let adjacent = if previous_index == index {
                    left.end() == event.start
                } else {
                    previous_index + 1 == index
                        && left.end() == part.measures[previous_index].length
                        && event.start.is_zero()
                };
                if adjacent && left.voice != event.voice {
                    let outgoing = left.notes.iter().any(|note| has_tie(note, "start"));
                    let incoming = event.notes.iter().any(|note| has_tie(note, "stop"));
                    if outgoing || incoming {
                        if configured
                            && [&left.notes, &event.notes].iter().any(|notes| {
                                notes.iter().all(|note| note.get_child("rest").is_some())
                            })
                        {
                            for note in &mut left.notes {
                                set_tie(note, "start", false);
                            }
                            for note in &mut event.notes {
                                set_tie(note, "stop", false);
                            }
                            return Ok(());
                        }
                        let left_pitches =
                            left.notes.iter().map(pitch).collect::<Result<Vec<_>>>()?;
                        let right_pitches =
                            event.notes.iter().map(pitch).collect::<Result<Vec<_>>>()?;
                        if left.notes.len() == 1
                            && event.notes.len() == 1
                            && left_pitches != right_pitches
                        {
                            set_tie(&mut left.notes[0], "start", false);
                            set_tie(&mut event.notes[0], "stop", false);
                            if left_pitches[0].is_some() && right_pitches[0].is_some() {
                                let number = slur_number.as_ref().ok_or_else(|| {
                                    ScoreError::new(
                                        "No free slur number at a shared voice transition",
                                    )
                                })?;
                                for (note, kind) in
                                    [(&mut left.notes[0], "start"), (&mut event.notes[0], "stop")]
                                {
                                    if note.get_child("notations").is_none() {
                                        insert_note_child(note, Element::new("notations"));
                                    }
                                    let mut slur = Element::new("slur");
                                    slur.attributes.insert("type".into(), kind.into());
                                    slur.attributes.insert("number".into(), number.clone());
                                    append(note.get_mut_child("notations").unwrap(), slur);
                                }
                            }
                        } else {
                            for (a, note) in left.notes.iter_mut().enumerate() {
                                for (b, next) in event.notes.iter_mut().enumerate() {
                                    if left_pitches[a].is_some()
                                        && left_pitches[a] == right_pitches[b]
                                        && (has_tie(note, "start") || has_tie(next, "stop"))
                                    {
                                        set_tie(note, "start", true);
                                        set_tie(next, "stop", true);
                                    }
                                }
                            }
                            let unmatched = left.notes.iter().enumerate().any(|(i, n)| {
                                has_tie(n, "start") && !right_pitches.contains(&left_pitches[i])
                            }) || event.notes.iter().enumerate().any(|(i, n)| {
                                has_tie(n, "stop") && !left_pitches.contains(&right_pitches[i])
                            });
                            if unmatched {
                                return Err(ScoreError::new(
                                    "ambiguous tie assignment when splitting",
                                ));
                            }
                        }
                    }
                }
                Ok(())
            })();
            if let Err(error) = checked {
                let context = format!(
                    "{}; from {}",
                    event.context(&part.measures[index]),
                    left.context(&part.measures[previous_index])
                );
                errors.push(ScoreError::new(format!("{context}: {error}")));
            }
        }
        previous = Some((index, event));
    }
    check_errors(errors)?;
    Ok(slur_number)
}

// A fallback can leave one endpoint in an omitted measure; retain only complete relations.
fn repair_fallback_relationships(
    measures: &mut [(Vec<Event>, Vec<TimedElement>)],
    shared_slur: Option<&str>,
) -> Result<()> {
    let mut ties = HashMap::new();
    let mut slurs: HashMap<(String, String), Vec<(usize, String)>> = HashMap::new();
    let mut keep_ties = HashSet::new();
    let mut keep_slurs = HashSet::new();
    let mut index = 0;
    for (events, _) in &*measures {
        for event in events {
            let mut next_ties = HashMap::new();
            for note in &event.notes {
                if let Some(pitch) = pitch(note)? {
                    if has_tie(note, "stop") {
                        if let Some(previous) = ties.get(&pitch) {
                            keep_ties.insert((*previous, "start"));
                            keep_ties.insert((index, "stop"));
                        }
                    }
                    if has_tie(note, "start") {
                        next_ties.insert(pitch, index);
                    }
                }
                if let Some(notations) = note.get_child("notations") {
                    for slur in elements(notations).filter(|node| node.name == "slur") {
                        let number = slur
                            .attributes
                            .get("number")
                            .cloned()
                            .unwrap_or_else(|| "1".into());
                        let kind = slur.attributes.get("type").cloned().unwrap_or_default();
                        let voice = if shared_slur == Some(number.as_str()) {
                            String::new()
                        } else {
                            event.voice.clone()
                        };
                        let key = (voice, number.clone());
                        if kind == "start" {
                            slurs.insert(key, vec![(index, kind)]);
                        } else if kind == "continue" {
                            if let Some(pending) = slurs.get_mut(&key) {
                                pending.push((index, kind));
                            }
                        } else if kind == "stop" {
                            if let Some(pending) = slurs.remove(&key) {
                                for (note, kind) in pending {
                                    keep_slurs.insert((note, number.clone(), kind));
                                }
                                keep_slurs.insert((index, number, kind));
                            }
                        }
                    }
                }
                index += 1;
            }
            if !event.grace() {
                ties = next_ties;
            }
        }
    }
    for (index, note) in measures
        .iter_mut()
        .flat_map(|(events, _)| events)
        .flat_map(|event| &mut event.notes)
        .enumerate()
    {
        for kind in ["start", "stop"] {
            if !keep_ties.contains(&(index, kind)) {
                set_tie(note, kind, false);
            }
        }
        if let Some(notations) = note.get_mut_child("notations") {
            notations.children.retain(|node| {
                node.as_element().is_none_or(|mark| {
                    mark.name != "slur"
                        || keep_slurs.contains(&(
                            index,
                            mark.attributes
                                .get("number")
                                .cloned()
                                .unwrap_or_else(|| "1".into()),
                            mark.attributes.get("type").cloned().unwrap_or_default(),
                        ))
                })
            });
        }
    }
    Ok(())
}

fn check_relationships(
    part: &Part,
    assignments: &Assignments,
    slots: &[AllocatedLane],
    configured: bool,
) -> Result<()> {
    let mut ties = HashMap::new();
    let mut slurs = HashMap::new();
    let mut errors = Vec::new();
    for (measure_index, measure) in part.measures.iter().enumerate() {
        let mut events: Vec<_> = measure.events.iter().collect();
        events.sort_by(|a, b| a.start.cmp(&b.start));
        for event in events {
            for note in &event.notes {
                let lanes = assignments
                    .get(&(note as *const Element as usize))
                    .cloned()
                    .unwrap_or_default();
                if lanes.is_empty() {
                    continue;
                }
                let pitch = note
                    .get_child("pitch")
                    .map(|p| {
                        Ok::<_, ScoreError>((
                            child_text(p, "step"),
                            numeric(&child_text(p, "alter").unwrap_or_else(|| "0".into()))?,
                            child_text(p, "octave"),
                        ))
                    })
                    .transpose();
                let pitch = match pitch {
                    Ok(pitch) => pitch,
                    Err(error) => {
                        errors.push(ScoreError::new(format!(
                            "{}: {error}",
                            event.context(measure)
                        )));
                        continue;
                    }
                };
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
                    && ties.remove(&key).is_some_and(|previous: BTreeSet<usize>| {
                        // Shared lanes may join or leave; native chord lanes must stay assigned.
                        previous
                            .symmetric_difference(&lanes)
                            .any(|lane| slots[*lane - 1].native(&event.voice, measure_index))
                    })
                {
                    errors.push(ScoreError::new(format!(
                        "{}: ambiguous tie assignment when splitting",
                        event.context(measure)
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
                            && slurs.remove(&key).is_some_and(|previous: BTreeSet<usize>| {
                                previous.symmetric_difference(&lanes).any(|lane| {
                                    let slot = &slots[*lane - 1];
                                    if configured {
                                        slot.native(&event.voice, measure_index)
                                    } else {
                                        slot.shared_voices.is_empty()
                                            || slot.slot.voice != event.voice
                                            || slot.native(&event.voice, measure_index)
                                    }
                                })
                            })
                        {
                            errors.push(ScoreError::new(format!(
                                "{}: ambiguous slur assignment when splitting (slur {})",
                                event.context(measure),
                                key.1
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
    check_errors(errors)
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
    split_score_inner(score, mode, selected, None)
}

/// Split using explicit measure-level fallback choices instead of legacy voice sharing.
/// Supply exactly one choice per preview lane in every selected part.
/// `None` means silence when all native sources for the staff are absent.
pub fn split_score_with_fallbacks(
    score: &Score,
    mode: &str,
    selected: Option<&[String]>,
    fallbacks: &[SplitFallback],
) -> Result<Element> {
    split_score_inner(score, mode, selected, Some(fallbacks))
}

fn split_score_inner(
    score: &Score,
    mode: &str,
    selected: Option<&[String]>,
    fallbacks: Option<&[SplitFallback]>,
) -> Result<Element> {
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
    let mut plans = HashMap::new();
    for part in &score.parts {
        if selected.is_none_or(|ids| ids.iter().any(|id| id == part.id())) {
            plans.insert(part.id(), allocation_plan(part, mode)?);
        }
    }
    if let Some(fallbacks) = fallbacks {
        let mut seen = HashSet::new();
        for choice in fallbacks {
            let key = (choice.part_id.as_str(), choice.voice.as_str(), choice.tone);
            if !seen.insert(key) {
                return Err(ScoreError::new("Repeated fallback lane setting"));
            }
            let slots = plans.get_mut(choice.part_id.as_str()).ok_or_else(|| {
                ScoreError::new("Fallback setting refers to an unknown or unselected part")
            })?;
            let part = score
                .parts
                .iter()
                .find(|part| part.id() == choice.part_id)
                .unwrap();
            let voices: HashSet<_> = part
                .measures
                .iter()
                .flat_map(|measure| &measure.events)
                .map(|event| event.voice.as_str())
                .collect();
            if choice
                .fallback_voice
                .as_ref()
                .is_some_and(|voice| voice == &choice.voice || !voices.contains(voice.as_str()))
            {
                return Err(ScoreError::new(
                    "Fallback voice must be another source voice in the part",
                ));
            }
            let slot = slots
                .iter_mut()
                .find(|lane| lane.slot.voice == choice.voice && lane.slot.tone == choice.tone)
                .ok_or_else(|| ScoreError::new("Unknown fallback lane"))?;
            slot.slot.fallback_voice = choice.fallback_voice.clone();
        }
        if plans.values().map(Vec::len).sum::<usize>() != seen.len() {
            return Err(ScoreError::new("Missing fallback lane settings"));
        }
    }
    let mut used: HashSet<_> = descendants(&score.root)
        .iter()
        .filter_map(|node| node.attributes.get("id").cloned())
        .collect();
    let mut replacements = HashMap::new();
    let mut errors = Vec::new();
    for part in &score.parts {
        if selected.is_some_and(|ids| !ids.iter().any(|id| id == part.id())) {
            continue;
        }
        let slots = &plans[part.id()];
        if slots.len() < 2 {
            continue;
        }
        let mut assignments = HashMap::new();
        let mut copies = vec![(part.definition.clone(), part.source.clone())];
        for (index, slot) in slots.iter().enumerate() {
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
            let mut measures = Vec::new();
            let mut failed = false;
            for measure in &part.measures {
                match lane_measure(
                    measure,
                    slot,
                    mode,
                    &mut assignments,
                    lane,
                    fallbacks.is_some(),
                ) {
                    Ok(result) => measures.push(result),
                    Err(error) => {
                        errors.push(error);
                        failed = true;
                    }
                }
            }
            if failed {
                continue;
            }
            if !slot.shared_voices.is_empty() {
                // Optional unison copies can lose an endpoint when another native voice takes over.
                if let Err(error) = repair_fallback_relationships(&mut measures, None) {
                    errors.push(error);
                }
            }
            let shared_slur = match connect_shared_voices(part, &mut measures, fallbacks.is_some())
            {
                Ok(number) => number,
                Err(error) => {
                    errors.push(error);
                    None
                }
            };
            if fallbacks.is_some() {
                if let Err(error) =
                    repair_fallback_relationships(&mut measures, shared_slur.as_deref())
                {
                    errors.push(error);
                }
            }
            for (measure, (mut events, extras)) in part.measures.iter().zip(measures) {
                for event in &mut events {
                    event.voice = "1".into();
                }
                match render_measure(measure, &events, Some(&extras), None, None) {
                    Ok(rendered) => append(&mut derived, rendered),
                    Err(error) => errors.push(error),
                }
            }
            copy_ids(&mut definition, &mut derived, &new_id, &mut used);
            copies.push((definition, derived));
        }
        if let Err(error) = check_relationships(part, &assignments, slots, fallbacks.is_some()) {
            errors.push(error);
        }
        replacements.insert(part.id().to_owned(), copies);
    }
    check_errors(errors)?;
    Ok(build_score(score, &replacements))
}
