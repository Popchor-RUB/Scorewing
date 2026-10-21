pub use crate::xml::*;
pub use crate::{Result, ScoreError};
use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{One, Signed, Zero};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use xmltree::{Element, XMLNode};

pub type Fraction = num_rational::BigRational;
pub fn frac(value: i64) -> Fraction {
    Fraction::from_integer(value.into())
}

#[derive(Debug, Clone)]
pub struct Event {
    pub start: Fraction,
    pub duration: Fraction,
    pub notes: Vec<Element>,
    pub voice: String,
}
impl Event {
    pub fn end(&self) -> Fraction {
        &self.start + &self.duration
    }
    pub fn grace(&self) -> bool {
        self.notes
            .first()
            .is_some_and(|n| child(n, "grace").is_some())
    }
}
#[derive(Debug, Clone)]
pub struct TimedElement {
    pub position: Fraction,
    pub element: Element,
    pub divisions: Fraction,
}
#[derive(Debug, Clone)]
pub struct Measure {
    pub source: Element,
    pub events: Vec<Event>,
    pub extras: Vec<TimedElement>,
    pub length: Fraction,
    pub divisions: Fraction,
    pub state: BTreeMap<String, Element>,
    pub context: String,
}
#[derive(Debug, Clone)]
pub struct Part {
    pub source: Element,
    pub definition: Element,
    pub measures: Vec<Measure>,
}
impl Part {
    pub fn id(&self) -> &str {
        self.source
            .attributes
            .get("id")
            .map(String::as_str)
            .unwrap_or("")
    }
    pub fn name(&self) -> String {
        child_text(&self.definition, "part-name").unwrap_or_else(|| self.id().to_owned())
    }
}
#[derive(Debug, Clone)]
pub struct Score {
    pub root: Element,
    pub parts: Vec<Part>,
}

pub fn compare_voices(a: &str, b: &str) -> Ordering {
    let number = |s: &str| {
        if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) {
            s.parse::<BigInt>().ok()
        } else {
            None
        }
    };
    match (number(a), number(b)) {
        (Some(x), Some(y)) => x.cmp(&y).then(a.cmp(b)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        _ => a.cmp(b),
    }
}

pub fn parse_fraction(text: &str) -> Result<Fraction> {
    let value = text.trim();
    let mut exponent_split = value.split(['e', 'E']);
    let mantissa = exponent_split.next().unwrap_or("");
    let exponent = exponent_split
        .next()
        .map(str::parse::<i32>)
        .transpose()
        .map_err(|_| ScoreError::new("expected a finite number"))?
        .unwrap_or(0);
    if exponent_split.next().is_some() || exponent.unsigned_abs() > 10000 {
        return Err(ScoreError::new("expected a finite number"));
    }
    let mut decimal = mantissa.split('.');
    let whole = decimal.next().unwrap_or("");
    let tail = decimal.next().unwrap_or("");
    if decimal.next().is_some() || (!tail.is_empty() && !tail.bytes().all(|c| c.is_ascii_digit())) {
        return Err(ScoreError::new("expected a finite number"));
    }
    let digits = format!("{whole}{tail}");
    let numerator = digits
        .parse::<BigInt>()
        .map_err(|_| ScoreError::new("expected a finite number"))?;
    let scale = tail.len() as i32 - exponent;
    Ok(if scale >= 0 {
        Fraction::new(numerator, BigInt::from(10).pow(scale as u32))
    } else {
        Fraction::from_integer(numerator * BigInt::from(10).pow((-scale) as u32))
    })
}
pub fn quantity(text: &str, context: &str, positive: bool) -> Result<Fraction> {
    let value = parse_fraction(text)
        .map_err(|_| ScoreError(format!("{context}: expected a finite number, got {text:?}")))?;
    if value.is_negative() || positive && value.is_zero() {
        return Err(ScoreError(format!(
            "{context}: value must be {}",
            if positive { "positive" } else { "nonnegative" }
        )));
    }
    Ok(value)
}
pub fn parse_score(root: Element) -> Result<Score> {
    if root.name != "score-partwise" || root.namespace.is_some() {
        return Err(ScoreError::new("Only score-partwise MusicXML is supported; export score-timewise as score-partwise first"));
    }
    let definitions: Vec<_> = child(&root, "part-list")
        .map(|p| children(p, "score-part").cloned().collect())
        .unwrap_or_default();
    let sources: Vec<_> = children(&root, "part").cloned().collect();
    let ids: Vec<_> = definitions.iter().map(|p| p.attributes.get("id")).collect();
    if ids.is_empty()
        || ids.iter().any(Option::is_none)
        || ids.iter().collect::<HashSet<_>>().len() != ids.len()
    {
        return Err(ScoreError::new("part-list must contain distinct part IDs"));
    }
    if sources
        .iter()
        .map(|p| p.attributes.get("id"))
        .collect::<Vec<_>>()
        != ids
    {
        return Err(ScoreError::new(
            "part-list and score parts must have the same IDs in the same order",
        ));
    }
    let mut parts = Vec::new();
    for (source, definition) in sources.into_iter().zip(definitions) {
        let name = child_text(&definition, "part-name")
            .unwrap_or_else(|| source.attributes.get("id").cloned().unwrap_or_default());
        let mut multi = false;
        visit(&source, &mut |node| {
            multi |= matches!(node.name.as_str(), "staves" | "staff") && text(node).trim() != "1";
            multi |= matches!(
                node.name.as_str(),
                "clef" | "key" | "time" | "staff-details" | "transpose" | "measure-style"
            ) && node.attributes.get("number").is_some_and(|n| n != "1");
        });
        if multi {
            return Err(ScoreError(format!(
                "Part {name:?}: multi-staff instruments are outside the supported scope"
            )));
        }
        let mut divisions = frac(1);
        let mut state = BTreeMap::new();
        let mut measures = Vec::new();
        for measure in children(&source, "measure") {
            let context = format!(
                "Part {name:?}, measure {}",
                measure
                    .attributes
                    .get("number")
                    .map(String::as_str)
                    .unwrap_or("?")
            );
            let parsed = parse_measure(measure, &mut divisions, &mut state, context)?;
            measures.push(parsed);
        }
        if measures.is_empty() {
            return Err(ScoreError(format!("Part {name:?}: no measures")));
        }
        parts.push(Part {
            source,
            definition,
            measures,
        });
    }
    Ok(Score { root, parts })
}
fn parse_measure(
    source: &Element,
    divisions: &mut Fraction,
    state: &mut BTreeMap<String, Element>,
    context: String,
) -> Result<Measure> {
    let mut cursor = frac(0);
    let mut extent = frac(0);
    let mut events: Vec<Event> = Vec::new();
    let mut extras = Vec::new();
    let mut previous: Option<usize> = None;
    let mut start_state = state.clone();
    for node in elements(source) {
        let location = format!("{context}, position {cursor}");
        match node.name.as_str() {
            "attributes" => {
                if let Some(d) = child_text(node, "divisions") {
                    *divisions = quantity(&d, &format!("{location} divisions"), true)?;
                }
                for attribute in elements(node).filter(|a| a.name != "divisions") {
                    state.insert(attribute.name.clone(), attribute.clone());
                }
                if cursor.is_zero() {
                    start_state = state.clone();
                }
                extras.push(TimedElement {
                    position: cursor.clone(),
                    element: node.clone(),
                    divisions: divisions.clone(),
                });
            }
            "backup" | "forward" => {
                let duration = quantity(
                    &child_text(node, "duration").unwrap_or_default(),
                    &location,
                    true,
                )? / &*divisions;
                if node.name == "backup" {
                    cursor -= duration;
                } else {
                    cursor += duration;
                }
                if cursor.is_negative() {
                    return Err(ScoreError(format!(
                        "{location}: backup crosses the beginning of the measure"
                    )));
                }
                extent = extent.max(cursor.clone());
                previous = None;
            }
            "note" => {
                let grace = child(node, "grace").is_some();
                let duration = if grace {
                    frac(0)
                } else {
                    quantity(
                        &child_text(node, "duration").unwrap_or_default(),
                        &location,
                        true,
                    )? / &*divisions
                };
                let voice = child_text(node, "voice").unwrap_or_else(|| "1".into());
                if child(node, "chord").is_some() {
                    let index = previous.ok_or_else(|| {
                        ScoreError(format!(
                            "{location}: chord tones must share a voice, onset, and duration"
                        ))
                    })?;
                    if events[index].voice != voice
                        || events[index].duration != duration
                        || events[index].grace() != grace
                    {
                        return Err(ScoreError(format!(
                            "{location}: chord tones must share a voice, onset, and duration"
                        )));
                    }
                    events[index].notes.push(node.clone());
                } else {
                    previous = Some(events.len());
                    events.push(Event {
                        start: cursor.clone(),
                        duration: duration.clone(),
                        notes: vec![node.clone()],
                        voice,
                    });
                    cursor += duration;
                }
                extent = extent.max(cursor.clone());
            }
            _ => extras.push(TimedElement {
                position: cursor.clone(),
                element: node.clone(),
                divisions: divisions.clone(),
            }),
        }
    }
    for voice in events
        .iter()
        .map(|e| e.voice.clone())
        .collect::<BTreeSet<_>>()
    {
        let mut end = frac(0);
        let mut stream: Vec<_> = events
            .iter()
            .filter(|e| e.voice == voice && !e.grace())
            .collect();
        stream.sort_by(|a, b| a.start.cmp(&b.start));
        for event in stream {
            if event.start < end {
                return Err(ScoreError(format!(
                    "{context}, position {}: overlapping notes in voice {voice}; use chord markers",
                    event.start
                )));
            }
            end = event.end();
        }
    }
    if extent.is_zero() {
        if let Some(time) = state.get("time") {
            let beats: Vec<_> = children(time, "beats").collect();
            let types: Vec<_> = children(time, "beat-type").collect();
            if beats.len() != types.len() {
                return Err(ScoreError(format!("{context}: invalid time signature")));
            }
            for (beat, kind) in beats.iter().zip(types) {
                let denominator = quantity(&text(kind), &context, true)?;
                for value in text(beat).split('+') {
                    extent += quantity(value, &context, false)? * frac(4) / &denominator;
                }
            }
        }
    }
    Ok(Measure {
        source: source.clone(),
        events,
        extras,
        length: extent,
        divisions: divisions.clone(),
        state: start_state,
        context,
    })
}

pub fn set_written_duration(note: &mut Element, duration: Fraction) -> Result<()> {
    let ratio = if let Some(modification) = child(note, "time-modification") {
        let actual = child_text(modification, "actual-notes")
            .unwrap_or_default()
            .parse::<BigInt>()
            .map_err(|_| ScoreError::new("Invalid tuplet time-modification"))?;
        let normal = child_text(modification, "normal-notes")
            .unwrap_or_default()
            .parse::<BigInt>()
            .map_err(|_| ScoreError::new("Invalid tuplet time-modification"))?;
        if normal.is_zero() {
            return Err(ScoreError::new("Invalid tuplet time-modification"));
        }
        Fraction::new(actual, normal)
    } else {
        frac(1)
    };
    for (i, name) in [
        "maxima", "long", "breve", "whole", "half", "quarter", "eighth", "16th", "32nd", "64th",
        "128th", "256th", "512th", "1024th",
    ]
    .iter()
    .enumerate()
    {
        let base = Fraction::new(32.into(), BigInt::one() << i);
        for dots in 0..4 {
            if &base * (frac(2) - Fraction::new(1.into(), BigInt::one() << dots))
                == &duration * &ratio
            {
                set_child(note, "type", *name);
                remove_children(note, "dot");
                for _ in 0..dots {
                    insert_note_child(note, Element::new("dot"));
                }
                return Ok(());
            }
        }
    }
    set_child(note, "type", "quarter");
    remove_children(note, "dot");
    remove_children(note, "time-modification");
    let mut modification = Element::new("time-modification");
    append(
        &mut modification,
        text_element("actual-notes", duration.denom().to_string()),
    );
    append(
        &mut modification,
        text_element("normal-notes", duration.numer().to_string()),
    );
    insert_note_child(note, modification);
    Ok(())
}
pub fn rest_event(start: Fraction, duration: Fraction, voice: &str) -> Result<Event> {
    let mut note = Element::new("note");
    append(&mut note, Element::new("rest"));
    set_written_duration(&mut note, duration.clone())?;
    Ok(Event {
        start,
        duration,
        notes: vec![note],
        voice: voice.into(),
    })
}
pub fn slice_event(event: &Event, start: Fraction, end: Fraction) -> Result<Event> {
    if !(event.start <= start && start < end && end <= event.end()) {
        return Err(ScoreError::new("Invalid event slice"));
    }
    let mut notes = event.notes.clone();
    if start == event.start && end == event.end() {
        return Ok(Event {
            start: start.clone(),
            duration: end - start,
            notes,
            voice: event.voice.clone(),
        });
    }
    for note in &mut notes {
        set_written_duration(note, &end - &start)?;
        remove_children(note, "beam");
        let ties: HashMap<_, _> = children(note, "tie")
            .map(|t| {
                (
                    t.attributes.get("type").cloned().unwrap_or_default(),
                    t.clone(),
                )
            })
            .collect();
        let tied: HashMap<_, _> = child(note, "notations")
            .map(|n| {
                children(n, "tied")
                    .map(|t| {
                        (
                            t.attributes.get("type").cloned().unwrap_or_default(),
                            t.clone(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        remove_children(note, "tie");
        if let Some(notations) = child_mut(note, "notations") {
            notations.children.retain(|node| {
                node.as_element().is_none_or(|c| {
                    c.name != "tied" && (start == event.start || c.name == "tuplet")
                })
            });
        }
        if start != event.start {
            remove_children(note, "lyric");
        }
        if child(note, "rest").is_none() {
            for kind in ["stop", "start"] {
                let needed = if kind == "stop" {
                    start > event.start || ties.contains_key(kind)
                } else {
                    end < event.end() || ties.contains_key(kind)
                };
                if !needed {
                    continue;
                }
                let original =
                    kind == "stop" && start == event.start || kind == "start" && end == event.end();
                let mut tie = Element::new("tie");
                tie.attributes.insert("type".into(), kind.into());
                insert_note_child(
                    note,
                    if original {
                        ties.get(kind).cloned().unwrap_or(tie)
                    } else {
                        tie
                    },
                );
                if child(note, "notations").is_none() {
                    insert_note_child(note, Element::new("notations"));
                }
                let mut mark = Element::new("tied");
                mark.attributes.insert("type".into(), kind.into());
                append(
                    child_mut(note, "notations").unwrap(),
                    if original {
                        tied.get(kind).cloned().unwrap_or(mark)
                    } else {
                        mark
                    },
                );
            }
        }
    }
    Ok(Event {
        start: start.clone(),
        duration: end - start,
        notes,
        voice: event.voice.clone(),
    })
}

fn move_cursor(
    result: &mut Element,
    cursor: &mut Fraction,
    position: &Fraction,
    voice: Option<&str>,
    divisions: &BigInt,
) {
    let delta = position - &*cursor;
    if !delta.is_zero() {
        let mut node = Element::new(if delta.is_negative() {
            "backup"
        } else {
            "forward"
        });
        append(
            &mut node,
            text_element(
                "duration",
                (delta.abs() * Fraction::from_integer(divisions.clone()))
                    .to_integer()
                    .to_string(),
            ),
        );
        if delta.is_positive() {
            if let Some(voice) = voice {
                append(&mut node, text_element("voice", voice));
            }
        }
        append(result, node);
    }
    *cursor = position.clone();
}
fn emit_extra(
    result: &mut Element,
    cursor: &mut Fraction,
    extra: &TimedElement,
    divisions: &BigInt,
) -> Result<()> {
    move_cursor(result, cursor, &extra.position, None, divisions);
    let mut node = extra.element.clone();
    strip_position(&mut node);
    if node.name == "attributes" {
        remove_children(&mut node, "divisions");
        if elements(&node).next().is_none() {
            return Ok(());
        }
    }
    for offset in children_mut(&mut node, "offset") {
        let value = parse_fraction(&text(offset))? * Fraction::from_integer(divisions.clone())
            / &extra.divisions;
        set_text(offset, value.to_integer().to_string());
    }
    append(result, node);
    Ok(())
}
pub fn render_measure(
    measure: &Measure,
    events: &[Event],
    extras: Option<&[TimedElement]>,
    length: Option<&Fraction>,
    divisions: Option<&BigInt>,
) -> Result<Element> {
    let extras = extras.unwrap_or(&measure.extras);
    let length = length.unwrap_or(&measure.length);
    let mut divisions = divisions
        .cloned()
        .unwrap_or_else(BigInt::one)
        .lcm(measure.divisions.numer());
    divisions = divisions.lcm(length.denom());
    for event in events {
        divisions = divisions
            .lcm(event.start.denom())
            .lcm(event.duration.denom());
    }
    for extra in extras {
        divisions = divisions.lcm(extra.position.denom());
        for offset in children(&extra.element, "offset") {
            divisions = divisions.lcm((parse_fraction(&text(offset))? / &extra.divisions).denom());
        }
    }
    let mut result = Element::new("measure");
    result.attributes = measure.source.attributes.clone();
    result.attributes.remove("width");
    result.children.extend(
        measure
            .source
            .children
            .iter()
            .filter(|n| matches!(n, XMLNode::Comment(_) | XMLNode::ProcessingInstruction(..)))
            .cloned(),
    );
    let mut attributes = Element::new("attributes");
    append(
        &mut attributes,
        text_element("divisions", divisions.to_string()),
    );
    append(&mut result, attributes);
    let mut cursor = frac(0);
    let (right, mut pending): (Vec<_>, Vec<_>) = extras.iter().partition(|x| {
        x.element.name == "barline"
            && x.element
                .attributes
                .get("location")
                .is_none_or(|v| v == "right")
    });
    pending.sort_by(|a, b| a.position.cmp(&b.position));
    let mut pending_index = 0;
    let mut voices: Vec<_> = events
        .iter()
        .map(|e| e.voice.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    voices.sort_by(|a, b| compare_voices(a, b));
    for (track, voice) in voices.iter().enumerate() {
        move_cursor(&mut result, &mut cursor, &frac(0), None, &divisions);
        let mut stream: Vec<_> = events.iter().filter(|e| &e.voice == voice).collect();
        stream.sort_by(|a, b| a.start.cmp(&b.start));
        for event in stream {
            if track == 0 {
                while pending_index < pending.len()
                    && pending[pending_index].position <= event.start
                {
                    emit_extra(&mut result, &mut cursor, pending[pending_index], &divisions)?;
                    pending_index += 1;
                }
            }
            if event.start.is_negative() || event.end() > *length {
                return Err(ScoreError(format!(
                    "{}, position {}: event outside measure",
                    measure.context, event.start
                )));
            }
            move_cursor(
                &mut result,
                &mut cursor,
                &event.start,
                Some(voice),
                &divisions,
            );
            for (index, original) in event.notes.iter().enumerate() {
                let mut note = original.clone();
                strip_position(&mut note);
                remove_children(&mut note, "chord");
                if index > 0 {
                    insert_note_child(&mut note, Element::new("chord"));
                }
                if !event.grace() {
                    set_child(
                        &mut note,
                        "duration",
                        (&event.duration * Fraction::from_integer(divisions.clone()))
                            .to_integer()
                            .to_string(),
                    );
                }
                set_child(&mut note, "voice", voice);
                remove_children(&mut note, "staff");
                append(&mut result, note);
            }
            if !event.grace() {
                cursor = event.end();
            }
        }
        if track == 0 {
            while pending_index < pending.len() {
                emit_extra(&mut result, &mut cursor, pending[pending_index], &divisions)?;
                pending_index += 1;
            }
        }
        move_cursor(&mut result, &mut cursor, length, Some(voice), &divisions);
    }
    if voices.is_empty() {
        for extra in pending {
            emit_extra(&mut result, &mut cursor, extra, &divisions)?;
        }
        move_cursor(&mut result, &mut cursor, length, None, &divisions);
    }
    for extra in right {
        append(&mut result, extra.element.clone());
    }
    Ok(result)
}
pub fn build_score(
    score: &Score,
    replacements: &HashMap<String, Vec<(Element, Element)>>,
) -> Element {
    let mut root = score.root.clone();
    if let Some(list) = child_mut(&mut root, "part-list") {
        list.children = list
            .children
            .iter()
            .flat_map(|node| {
                if let Some(e) = node.as_element().filter(|e| e.name == "score-part") {
                    if let Some(replacement) =
                        e.attributes.get("id").and_then(|id| replacements.get(id))
                    {
                        return replacement
                            .iter()
                            .map(|(definition, _)| XMLNode::Element(definition.clone()))
                            .collect::<Vec<_>>();
                    }
                }
                vec![node.clone()]
            })
            .collect();
        let mut open: HashMap<String, (usize, usize)> = HashMap::new();
        let mut count = 0;
        let mut remove = HashSet::new();
        for (index, node) in list.children.iter().enumerate() {
            if let Some(e) = node.as_element() {
                if e.name == "score-part" {
                    count += 1;
                } else if e.name == "part-group" {
                    let number = e
                        .attributes
                        .get("number")
                        .cloned()
                        .unwrap_or_else(|| "1".into());
                    match e.attributes.get("type").map(String::as_str) {
                        Some("start") => {
                            open.insert(number, (index, count));
                        }
                        Some("stop") => {
                            if let Some((first, before)) = open.remove(&number) {
                                if before == count {
                                    remove.insert(first);
                                    remove.insert(index);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        list.children = std::mem::take(&mut list.children)
            .into_iter()
            .enumerate()
            .filter_map(|(i, n)| (!remove.contains(&i)).then_some(n))
            .collect();
    }
    root.children = root
        .children
        .iter()
        .flat_map(|node| {
            if let Some(e) = node.as_element().filter(|e| e.name == "part") {
                if let Some(replacement) =
                    e.attributes.get("id").and_then(|id| replacements.get(id))
                {
                    return replacement
                        .iter()
                        .map(|(_, part)| XMLNode::Element(part.clone()))
                        .collect::<Vec<_>>();
                }
            }
            vec![node.clone()]
        })
        .collect();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    fn score(body: &str, divisions: &str) -> Score {
        let data=format!("<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>Voice</part-name></score-part></part-list><part id=\"P1\"><measure number=\"1\"><attributes><divisions>{divisions}</divisions></attributes>{body}</measure></part></score-partwise>");
        parse_score(Element::parse(data.as_bytes()).unwrap()).unwrap()
    }
    #[test]
    fn decimal_timing_is_exact() {
        let s=score("<note><pitch><step>C</step><octave>4</octave></pitch><duration>1.5</duration><type>quarter</type></note>","1.5");
        assert_eq!(s.parts[0].measures[0].events[0].duration, frac(1));
        let rendered = render_measure(
            &s.parts[0].measures[0],
            &s.parts[0].measures[0].events,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            child_text(child(&rendered, "note").unwrap(), "duration").unwrap(),
            "3"
        );
    }
    #[test]
    fn slicing_preserves_ties_and_lyrics() {
        let s=score("<note><pitch><step>C</step><octave>4</octave></pitch><duration>4</duration><type>whole</type><lyric><text>Ah</text></lyric></note>","1");
        let event = &s.parts[0].measures[0].events[0];
        let left = slice_event(event, frac(0), frac(1)).unwrap();
        let right = slice_event(event, frac(1), frac(4)).unwrap();
        assert_eq!(child_text(&left.notes[0], "type").unwrap(), "quarter");
        assert_eq!(child_text(&right.notes[0], "type").unwrap(), "half");
        assert!(child(&right.notes[0], "dot").is_some());
        assert!(child(&right.notes[0], "lyric").is_none());
        assert_eq!(
            child(&left.notes[0], "tie").unwrap().attributes["type"],
            "start"
        );
        assert_eq!(
            child(&right.notes[0], "tie").unwrap().attributes["type"],
            "stop"
        );
    }
    #[test]
    fn invalid_timing_is_contextual() {
        let e=Element::parse("<score-partwise><part-list><score-part id=\"P1\"><part-name>Voice</part-name></score-part></part-list><part id=\"P1\"><measure number=\"7\"><backup><duration>1</duration></backup></measure></part></score-partwise>".as_bytes()).unwrap();
        let error = parse_score(e).unwrap_err().to_string();
        assert!(error.contains("measure 7"));
        assert!(error.contains("backup crosses"));
    }
    #[test]
    fn backup_chord_and_divisions_roundtrip() {
        let mut s=score("<note><pitch><step>C</step><octave>4</octave></pitch><duration>2</duration><voice>1</voice></note><note><chord/><pitch><step>E</step><octave>4</octave></pitch><duration>2</duration><voice>1</voice></note><backup><duration>2</duration></backup><attributes><divisions>3</divisions></attributes><note><pitch><step>G</step><octave>4</octave></pitch><duration>3</duration><voice>2</voice></note><forward><duration>3</duration></forward>","2");
        let m = &s.parts[0].measures[0];
        assert_eq!(m.events.len(), 2);
        assert_eq!(m.events[0].notes.len(), 2);
        assert_eq!(m.events[0].duration, frac(1));
        assert_eq!(m.events[1].start, frac(0));
        assert_eq!(m.length, frac(2));
        let rendered = render_measure(m, &m.events, None, None, None).unwrap();
        let part = child_mut(&mut s.root, "part").unwrap();
        remove_children(part, "measure");
        append(part, rendered);
        let parsed = parse_score(s.root).unwrap();
        assert_eq!(parsed.parts[0].measures[0].events[0].duration, frac(1));
        assert_eq!(parsed.parts[0].measures[0].length, frac(2));
    }
    #[test]
    fn preserves_relationship_style_at_original_endpoints() {
        let s=score("<note><pitch><step>C</step><octave>4</octave></pitch><duration>2</duration><tie type=\"stop\"/><type>half</type><notations><tied type=\"stop\" orientation=\"under\"/><slur type=\"stop\" number=\"1\"/></notations></note>","1");
        let left = slice_event(&s.parts[0].measures[0].events[0], frac(0), frac(1)).unwrap();
        let notation = child(&left.notes[0], "notations").unwrap();
        assert_eq!(
            children(notation, "tied")
                .find(|t| t.attributes["type"] == "stop")
                .unwrap()
                .attributes["orientation"],
            "under"
        );
        assert!(child(notation, "slur").is_some());
        let right = slice_event(&s.parts[0].measures[0].events[0], frac(1), frac(2)).unwrap();
        assert!(child(child(&right.notes[0], "notations").unwrap(), "slur").is_none());
    }
    #[test]
    fn rejects_timewise_and_multistaff() {
        assert!(parse_score(Element::new("score-timewise"))
            .unwrap_err()
            .to_string()
            .contains("score-timewise"));
        let mut s = score("<note><rest/><duration>1</duration></note>", "1");
        let part = child_mut(&mut s.root, "part").unwrap();
        let measure = child_mut(part, "measure").unwrap();
        let attributes = child_mut(measure, "attributes").unwrap();
        append(attributes, text_element("staves", "2"));
        assert!(parse_score(s.root)
            .unwrap_err()
            .to_string()
            .contains("multi-staff"));
    }
}
