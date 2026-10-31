//! Musical comparison that annotates the newer XML without rebuilding it.
use crate::{
    model::{frac, parse_fraction, Fraction, Part, Score},
    xml::{child, child_text, elements, text},
    Element, Result, ScoreError,
};
use similar::{capture_diff_slices, Algorithm, DiffTag};
use std::collections::{BTreeSet, HashMap, HashSet};

pub struct DiffResult {
    pub root: Element,
    pub warnings: Vec<String>,
}

type Path = Vec<usize>;
type Moment = (usize, Fraction);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct NoteKey {
    pitch: String,
    duration: Fraction,
    voice: String,
    grace: bool,
    grace_length: String,
}

struct Item {
    key: String,
    path: Path,
}

struct Note {
    key: NoteKey,
    moment: Moment,
    path: Path,
    lyrics: Vec<Item>,
}

struct Symbol {
    item: Item,
    moment: Moment,
    voice: Option<String>,
    visible: bool,
}

struct Relation {
    kind: String,
    start: Option<usize>,
    end: Option<usize>,
    marks: Vec<Path>,
}

struct Music {
    notes: Vec<Note>,
    symbols: Vec<Symbol>,
    relations: Vec<Relation>,
}

fn number(node: &Element, name: &str, default: &str) -> Result<Fraction> {
    parse_fraction(&child_text(node, name).unwrap_or_else(|| default.into()))
}

fn pitch(note: &Element, transpose: &Fraction) -> Result<String> {
    if child(note, "rest").is_some() {
        return Ok("rest".into());
    }
    if let Some(pitch) = child(note, "pitch") {
        let step = match child_text(pitch, "step").as_deref() {
            Some("C") => 0,
            Some("D") => 2,
            Some("E") => 4,
            Some("F") => 5,
            Some("G") => 7,
            Some("A") => 9,
            Some("B") => 11,
            _ => return Err(ScoreError::new("Diff: invalid pitch step")),
        };
        let octave = child_text(pitch, "octave")
            .ok_or_else(|| ScoreError::new("Diff: missing pitch octave"))?;
        return Ok((frac(step)
            + parse_fraction(&octave)? * frac(12)
            + number(pitch, "alter", "0")?
            + transpose)
            .to_string());
    }
    if let Some(unpitched) = child(note, "unpitched") {
        return Ok(format!(
            "unpitched:{}:{}",
            child_text(unpitched, "display-step").unwrap_or_default(),
            child_text(unpitched, "display-octave").unwrap_or_default()
        ));
    }
    Err(ScoreError::new(
        "Diff: note has no pitch, rest, or unpitched value",
    ))
}

// Only musical fields enter fingerprints; exporter IDs and engraving never do.
fn content(node: &Element) -> String {
    let mut value = format!("<{}", node.name);
    for name in ["number", "name", "type", "text"] {
        if let Some(v) = node.attributes.get(name) {
            if name != "number" || node.name == "lyric" {
                value.push_str(&format!(" {name}={v:?}"));
            }
        }
    }
    value.push('>');
    let node_text = text(node);
    value.push_str(&format!(
        "{:?}",
        if matches!(node.name.as_str(), "text" | "elision" | "other-dynamics") {
            node_text.as_str()
        } else {
            node_text.trim()
        }
    ));
    for element in elements(node) {
        if !matches!(
            element.name.as_str(),
            "footnote" | "level" | "frame" | "end-line" | "end-paragraph"
        ) {
            value.push_str(&content(element));
        }
    }
    value.push_str(&format!("</{}>", node.name));
    value
}

fn at<'a>(mut node: &'a Element, path: &[usize]) -> &'a Element {
    for &index in path {
        node = node.children[index].as_element().unwrap();
    }
    node
}

fn collect_symbols(
    node: &Element,
    path: Path,
    moment: &Moment,
    voice: &Option<String>,
    out: &mut Vec<Symbol>,
) -> Result<()> {
    if matches!(node.name.as_str(), "harmony" | "dynamics" | "wedge")
        || node.name == "sound" && node.attributes.contains_key("dynamics")
    {
        let visible = node.name != "sound";
        let key = if visible {
            content(node)
        } else {
            format!("sound:{}", parse_fraction(&node.attributes["dynamics"])?)
        };
        out.push(Symbol {
            item: Item { key, path },
            moment: moment.clone(),
            voice: voice.clone(),
            visible,
        });
        return Ok(());
    }
    for (index, element) in node
        .children
        .iter()
        .enumerate()
        .filter_map(|(i, n)| n.as_element().map(|e| (i, e)))
    {
        let mut path = path.clone();
        path.push(index);
        collect_symbols(element, path, moment, voice, out)?;
    }
    Ok(())
}

fn music(part: &Part) -> Result<Music> {
    let mut result = Music {
        notes: Vec::new(),
        symbols: Vec::new(),
        relations: Vec::new(),
    };
    let mut divisions = frac(1);
    let mut transpose = frac(0);
    for (measure_index, (mi, measure)) in part
        .source
        .children
        .iter()
        .enumerate()
        .filter_map(|(i, n)| {
            n.as_element()
                .filter(|e| e.name == "measure")
                .map(|e| (i, e))
        })
        .enumerate()
    {
        let mut cursor = frac(0);
        let mut previous = frac(0);
        for (ni, node) in measure
            .children
            .iter()
            .enumerate()
            .filter_map(|(i, n)| n.as_element().map(|e| (i, e)))
        {
            let path = vec![mi, ni];
            match node.name.as_str() {
                "attributes" => {
                    if child(node, "divisions").is_some() {
                        divisions = number(node, "divisions", "1")?;
                    }
                    if let Some(t) = child(node, "transpose") {
                        transpose = number(t, "chromatic", "0")?
                            + number(t, "octave-change", "0")? * frac(12);
                    }
                }
                "backup" => cursor -= number(node, "duration", "0")? / &divisions,
                "forward" => cursor += number(node, "duration", "0")? / &divisions,
                "note" => {
                    let grace = child(node, "grace").is_some();
                    let duration = if grace {
                        frac(0)
                    } else {
                        number(node, "duration", "0")? / &divisions
                    };
                    let start = if child(node, "chord").is_some() {
                        previous.clone()
                    } else {
                        previous = cursor.clone();
                        cursor += &duration;
                        previous.clone()
                    };
                    let voice = child_text(node, "voice").unwrap_or_else(|| "1".into());
                    let moment = (measure_index, start);
                    let mut lyrics: Vec<_> = node
                        .children
                        .iter()
                        .enumerate()
                        .filter_map(|(i, n)| {
                            n.as_element().filter(|e| e.name == "lyric").map(|e| Item {
                                key: content(e),
                                path: vec![mi, ni, i],
                            })
                        })
                        .collect();
                    lyrics.sort_by(|a, b| a.key.cmp(&b.key));
                    collect_symbols(
                        node,
                        path.clone(),
                        &moment,
                        &Some(voice.clone()),
                        &mut result.symbols,
                    )?;
                    let grace_length = if grace {
                        elements(node)
                            .filter(|e| {
                                matches!(e.name.as_str(), "type" | "dot" | "time-modification")
                            })
                            .map(content)
                            .collect::<Vec<_>>()
                            .join("")
                    } else {
                        String::new()
                    };
                    result.notes.push(Note {
                        key: NoteKey {
                            pitch: pitch(node, &transpose)?,
                            duration,
                            voice,
                            grace,
                            grace_length,
                        },
                        moment,
                        path,
                        lyrics,
                    });
                }
                _ => {
                    let position = &cursor + number(node, "offset", "0")? / &divisions;
                    collect_symbols(
                        node,
                        path,
                        &(measure_index, position),
                        &child_text(node, "voice"),
                        &mut result.symbols,
                    )?;
                }
            }
        }
    }
    result
        .notes
        .sort_by(|a, b| a.moment.cmp(&b.moment).then(a.key.cmp(&b.key)));
    result
        .symbols
        .sort_by(|a, b| a.moment.cmp(&b.moment).then(a.item.key.cmp(&b.item.key)));
    let mut open: HashMap<String, (usize, Vec<Path>)> = HashMap::new();
    for (index, note) in result.notes.iter().enumerate() {
        let node = at(&part.source, &note.path);
        let mut marks = Vec::new();
        for (ci, element) in node
            .children
            .iter()
            .enumerate()
            .filter_map(|(i, n)| n.as_element().map(|e| (i, e)))
        {
            if element.name == "notations" {
                for (ri, relation) in element.children.iter().enumerate().filter_map(|(i, n)| {
                    n.as_element()
                        .filter(|e| matches!(e.name.as_str(), "tied" | "slur"))
                        .map(|e| (i, e))
                }) {
                    let mut path = note.path.clone();
                    path.extend([ci, ri]);
                    marks.push((relation, path));
                }
            }
        }
        if !marks.iter().any(|(r, _)| r.name == "tied") {
            for (ci, relation) in node
                .children
                .iter()
                .enumerate()
                .filter_map(|(i, n)| n.as_element().filter(|e| e.name == "tie").map(|e| (i, e)))
            {
                let mut path = note.path.clone();
                path.push(ci);
                marks.push((relation, path));
            }
        }
        // A chain may stop and restart on the same note regardless of XML child order.
        marks.sort_by_key(|(r, _)| r.attributes.get("type").is_none_or(|t| t != "stop"));
        for (relation, path) in marks {
            let end_type = relation
                .attributes
                .get("type")
                .map(String::as_str)
                .unwrap_or("");
            let kind = if relation.name == "slur" {
                "slur"
            } else if end_type == "let-ring" {
                "let-ring tie"
            } else {
                "tie"
            };
            let key = if kind == "slur" {
                format!(
                    "slur:{}",
                    relation
                        .attributes
                        .get("number")
                        .map(String::as_str)
                        .unwrap_or("1")
                )
            } else {
                format!("{kind}:{}:{}", note.key.voice, note.key.pitch)
            };
            if matches!(end_type, "stop" | "continue") {
                let (start, mut paths) = open
                    .remove(&key)
                    .map(|(i, p)| (Some(i), p))
                    .unwrap_or((None, Vec::new()));
                paths.push(path.clone());
                result.relations.push(Relation {
                    kind: kind.into(),
                    start,
                    end: Some(index),
                    marks: paths,
                });
            }
            if matches!(end_type, "start" | "continue" | "let-ring") {
                if let Some((start, paths)) = open.insert(key, (index, vec![path])) {
                    result.relations.push(Relation {
                        kind: kind.into(),
                        start: Some(start),
                        end: None,
                        marks: paths,
                    });
                }
            }
        }
    }
    for (key, (start, paths)) in open {
        result.relations.push(Relation {
            kind: key.split(':').next().unwrap().into(),
            start: Some(start),
            end: None,
            marks: paths,
        });
    }
    result
        .relations
        .sort_by(|a, b| (a.start, a.end, &a.kind).cmp(&(b.start, b.end, &b.kind)));
    Ok(result)
}

fn mark_note(note: &Note, marked: &mut BTreeSet<Path>) {
    marked.insert(note.path.clone());
}

fn nearby(music: &Music, index: usize, marked: &mut BTreeSet<Path>) -> bool {
    if let Some(note) = music.notes.get(index).or_else(|| music.notes.last()) {
        mark_note(note, marked);
        true
    } else if let Some(symbol) = music.symbols.iter().find(|s| s.visible) {
        marked.insert(symbol.item.path.clone());
        true
    } else {
        false
    }
}

fn compare_items(
    old: &[Item],
    new: &[Item],
    fallback: &Note,
    marked: &mut BTreeSet<Path>,
    warnings: &mut Vec<String>,
) {
    let a: Vec<_> = old.iter().map(|i| &i.key).collect();
    let b: Vec<_> = new.iter().map(|i| &i.key).collect();
    for op in capture_diff_slices(Algorithm::Myers, &a, &b) {
        let (tag, old_range, new_range) = op.as_tag_tuple();
        let (old_len, start, new_len) = (old_range.len(), new_range.start, new_range.len());
        if tag != DiffTag::Equal {
            for item in &new[start..start + new_len] {
                marked.insert(item.path.clone());
            }
            if old_len > new_len {
                mark_note(fallback, marked);
                warnings.push("Deleted lyrics: the surviving note is marked red.".into());
            }
        }
    }
}

fn pair_notes(
    old: &Music,
    new: &Music,
    mapping: &mut [Option<usize>],
    marked: &mut BTreeSet<Path>,
    warnings: &mut Vec<String>,
) {
    let a: Vec<_> = old.notes.iter().map(|n| &n.key).collect();
    let b: Vec<_> = new.notes.iter().map(|n| &n.key).collect();
    for op in capture_diff_slices(Algorithm::Myers, &a, &b) {
        let (tag, old_range, new_range) = op.as_tag_tuple();
        let (oi, ol, ni, nl) = (
            old_range.start,
            old_range.len(),
            new_range.start,
            new_range.len(),
        );
        if tag == DiffTag::Equal {
            for offset in 0..ol {
                mapping[oi + offset] = Some(ni + offset);
            }
        } else {
            // Pitch anchors preserve correspondence when durations or voice assignments change.
            let pitches_a: Vec<_> = old.notes[oi..oi + ol]
                .iter()
                .map(|n| &n.key.pitch)
                .collect();
            let pitches_b: Vec<_> = new.notes[ni..ni + nl]
                .iter()
                .map(|n| &n.key.pitch)
                .collect();
            for sub in capture_diff_slices(Algorithm::Myers, &pitches_a, &pitches_b) {
                let (_, old_range, new_range) = sub.as_tag_tuple();
                let (x, xl, y, yl) = (
                    old_range.start,
                    old_range.len(),
                    new_range.start,
                    new_range.len(),
                );
                for offset in 0..xl.min(yl) {
                    mapping[oi + x + offset] = Some(ni + y + offset);
                }
                for note in &new.notes[ni + y..ni + y + yl] {
                    mark_note(note, marked);
                }
                if xl > yl {
                    let visible = nearby(new, ni + y + yl, marked);
                    warnings.push(format!(
                        "Deleted {} note(s)/rest(s): {}.",
                        xl - yl,
                        if visible {
                            "the nearest surviving music is marked red"
                        } else {
                            "no surviving music can be marked"
                        }
                    ));
                }
            }
            let repeated = (ol > nl && oi > 0 && a[oi..oi + ol].contains(&a[oi - 1]))
                || (nl > ol && ni > 0 && b[ni..ni + nl].contains(&b[ni - 1]));
            if repeated {
                warnings.push("Repeated music has ambiguous correspondence; alignment follows document order.".into());
            }
        }
    }
    for (oi, ni) in mapping
        .iter()
        .enumerate()
        .filter_map(|(i, n)| n.map(|n| (i, n)))
    {
        compare_items(
            &old.notes[oi].lyrics,
            &new.notes[ni].lyrics,
            &new.notes[ni],
            marked,
            warnings,
        );
    }
    let paired: HashSet<_> = mapping.iter().flatten().copied().collect();
    for (i, note) in new.notes.iter().enumerate() {
        if !paired.contains(&i) {
            for lyric in &note.lyrics {
                marked.insert(lyric.path.clone());
            }
        }
    }
}

fn anchor(symbol: &Symbol, notes: &[Note]) -> Option<(usize, Fraction)> {
    let first = notes.partition_point(|n| n.moment < symbol.moment);
    let candidate = notes[first..]
        .iter()
        .enumerate()
        .find(|(_, n)| symbol.voice.as_ref().is_none_or(|v| *v == n.key.voice))
        .map(|(i, n)| (first + i, n))
        .or_else(|| {
            notes
                .iter()
                .enumerate()
                .rev()
                .find(|(_, n)| symbol.voice.as_ref().is_none_or(|v| *v == n.key.voice))
        });
    candidate.map(|(i, n)| {
        (
            i,
            if n.moment.0 == symbol.moment.0 {
                &symbol.moment.1 - &n.moment.1
            } else {
                symbol.moment.1.clone()
            },
        )
    })
}

fn compare_symbols(
    old: &Music,
    new: &Music,
    mapping: &[Option<usize>],
    marked: &mut BTreeSet<Path>,
    warnings: &mut Vec<String>,
) {
    let keys = |music: &Music, older: bool| -> Vec<_> {
        music
            .symbols
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let target = anchor(s, &music.notes);
                let mapped = target
                    .as_ref()
                    .and_then(|(n, _)| if older { mapping[*n] } else { Some(*n) });
                // Unmapped old anchors must never accidentally equal a new anchor.
                (
                    s.item.key.clone(),
                    mapped,
                    target.as_ref().map(|(_, offset)| offset.clone()),
                    if older && mapped.is_none() && target.is_some() {
                        Some(i)
                    } else {
                        None
                    },
                    if target.is_none() {
                        Some(s.moment.clone())
                    } else {
                        None
                    },
                )
            })
            .collect()
    };
    let a = keys(old, true);
    let b = keys(new, false);
    for op in capture_diff_slices(Algorithm::Myers, &a, &b) {
        let (tag, old_range, new_range) = op.as_tag_tuple();
        let (oi, ol, ni, nl) = (
            old_range.start,
            old_range.len(),
            new_range.start,
            new_range.len(),
        );
        if tag == DiffTag::Equal {
            continue;
        }
        for symbol in &new.symbols[ni..ni + nl] {
            if symbol.visible {
                marked.insert(symbol.item.path.clone());
            } else if let Some((i, _)) = anchor(symbol, &new.notes) {
                nearby(new, i, marked);
            }
        }
        if ol > nl {
            for symbol in &old.symbols[oi + nl..oi + ol] {
                let target = anchor(symbol, &old.notes)
                    .and_then(|(i, _)| {
                        mapping[i..]
                            .iter()
                            .flatten()
                            .next()
                            .copied()
                            .or_else(|| mapping[..i].iter().rev().flatten().next().copied())
                    })
                    .unwrap_or(0);
                let visible = nearby(new, target, marked);
                warnings.push(format!(
                    "Deleted dynamics/chord symbol: {}.",
                    if visible {
                        "the nearest surviving music is marked red"
                    } else {
                        "no surviving music can be marked"
                    }
                ));
            }
        }
    }
}

fn mark_relation(
    relation: &Relation,
    music: &Music,
    source: &Element,
    marked: &mut BTreeSet<Path>,
) {
    for i in [relation.start, relation.end].into_iter().flatten() {
        mark_note(&music.notes[i], marked);
    }
    for path in &relation.marks {
        if at(source, path).name != "tie" {
            marked.insert(path.clone());
        }
    }
}

fn compare_relations(
    old: &Music,
    new: &Music,
    mapping: &[Option<usize>],
    source: &Element,
    marked: &mut BTreeSet<Path>,
    warnings: &mut Vec<String>,
) {
    let mut used = HashSet::new();
    let mut connections: HashMap<_, Vec<usize>> = HashMap::new();
    for (i, relation) in new.relations.iter().enumerate().rev() {
        connections
            .entry((relation.kind.as_str(), relation.start, relation.end))
            .or_default()
            .push(i);
    }
    for relation in &old.relations {
        let start = relation.start.and_then(|i| mapping[i]);
        let end = relation.end.and_then(|i| mapping[i]);
        let mapped = relation.start.is_none_or(|i| mapping[i].is_some())
            && relation.end.is_none_or(|i| mapping[i].is_some());
        let found = if mapped {
            connections
                .get_mut(&(relation.kind.as_str(), start, end))
                .and_then(Vec::pop)
        } else {
            None
        };
        if let Some(i) = found {
            used.insert(i);
        } else {
            let mut visible = false;
            for i in [start, end].into_iter().flatten() {
                visible |= nearby(new, i, marked);
            }
            if !visible {
                let old_index = relation.start.or(relation.end).unwrap_or(0);
                let target = mapping[old_index..]
                    .iter()
                    .flatten()
                    .next()
                    .copied()
                    .unwrap_or(new.notes.len());
                visible = nearby(new, target, marked);
            }
            warnings.push(format!(
                "Removed/changed {}: {}.",
                relation.kind,
                if visible {
                    "surviving endpoint or nearby notes are marked red"
                } else {
                    "no surviving music can be marked"
                }
            ));
        }
    }
    for (i, relation) in new.relations.iter().enumerate() {
        if !used.contains(&i) {
            mark_relation(relation, new, source, marked);
        }
    }
}

fn part_matches(old: &[Part], new: &[Part]) -> Result<Vec<Option<usize>>> {
    let mut matches = vec![None; new.len()];
    let mut used = HashSet::new();
    for (ni, n) in new.iter().enumerate() {
        let name = n.name();
        let candidates: Vec<_> = old
            .iter()
            .enumerate()
            .filter(|(_, o)| o.name() == name)
            .collect();
        if candidates.len() == 1 && new.iter().filter(|n| n.name() == name).count() == 1 {
            matches[ni] = Some(candidates[0].0);
            used.insert(candidates[0].0);
        }
    }
    for (ni, n) in new
        .iter()
        .enumerate()
        .filter(|(i, _)| matches[*i].is_none())
        .collect::<Vec<_>>()
    {
        if let Some((oi, _)) = old
            .iter()
            .enumerate()
            .find(|(i, o)| !used.contains(i) && o.id() == n.id())
        {
            matches[ni] = Some(oi);
            used.insert(oi);
        }
    }
    for (ni, n) in new.iter().enumerate() {
        if matches[ni].is_none()
            && old
                .iter()
                .enumerate()
                .any(|(i, o)| !used.contains(&i) && o.name() == n.name())
        {
            return Err(ScoreError::new(format!("Cannot unambiguously match instrument {:?}; use distinct part names or stable part IDs.", n.name())));
        }
    }
    Ok(matches)
}

fn apply_marks(root: &mut Element, marks: &BTreeSet<Path>) {
    for path in marks {
        let mut node = &mut *root;
        for &index in path {
            node = node.children[index].as_mut_element().unwrap();
        }
        node.attributes.insert("color".into(), "#FF0000".into());
    }
}

/// Return the newer score with musical changes colored red. Inputs are never modified.
pub fn diff_score(older: &Score, newer: &Score) -> Result<DiffResult> {
    let matches = part_matches(&older.parts, &newer.parts)?;
    let old: Vec<_> = older.parts.iter().map(music).collect::<Result<_>>()?;
    let new: Vec<_> = newer.parts.iter().map(music).collect::<Result<_>>()?;
    let mut marks = vec![BTreeSet::new(); new.len()];
    let mut warnings = Vec::new();
    for (ni, oi) in matches.iter().enumerate() {
        let before = warnings.len();
        if let Some(oi) = *oi {
            let mut mapping = vec![None; old[oi].notes.len()];
            pair_notes(
                &old[oi],
                &new[ni],
                &mut mapping,
                &mut marks[ni],
                &mut warnings,
            );
            compare_symbols(&old[oi], &new[ni], &mapping, &mut marks[ni], &mut warnings);
            compare_relations(
                &old[oi],
                &new[ni],
                &mapping,
                &newer.parts[ni].source,
                &mut marks[ni],
                &mut warnings,
            );
        } else {
            for note in &new[ni].notes {
                mark_note(note, &mut marks[ni]);
                for lyric in &note.lyrics {
                    marks[ni].insert(lyric.path.clone());
                }
            }
            for symbol in &new[ni].symbols {
                if symbol.visible {
                    marks[ni].insert(symbol.item.path.clone());
                }
            }
            for relation in &new[ni].relations {
                mark_relation(relation, &new[ni], &newer.parts[ni].source, &mut marks[ni]);
            }
        }
        for warning in &mut warnings[before..] {
            *warning = format!("{}: {warning}", newer.parts[ni].name());
        }
    }
    for (oi, part) in older.parts.iter().enumerate() {
        if matches.contains(&Some(oi)) {
            continue;
        }
        let target = matches
            .iter()
            .enumerate()
            .filter_map(|(ni, o)| o.map(|o| (ni, o)))
            .filter(|(_, o)| *o > oi)
            .min_by_key(|(_, o)| *o)
            .map(|(ni, _)| ni)
            .or_else(|| {
                matches
                    .iter()
                    .enumerate()
                    .filter_map(|(ni, o)| o.map(|o| (ni, o)))
                    .filter(|(_, o)| *o < oi)
                    .max_by_key(|(_, o)| *o)
                    .map(|(ni, _)| ni)
            })
            .or_else(|| (!new.is_empty()).then_some(0));
        let visible = target.is_some_and(|i| nearby(&new[i], 0, &mut marks[i]));
        warnings.push(format!(
            "Deleted instrument {:?}: {}.",
            part.name(),
            if visible {
                "the nearest surviving part's opening music is marked red"
            } else {
                "no surviving music can be marked"
            }
        ));
    }
    let mut root = newer.root.clone();
    for (i, part) in root
        .children
        .iter_mut()
        .filter_map(|n| n.as_mut_element())
        .filter(|e| e.name == "part")
        .enumerate()
    {
        apply_marks(part, &marks[i]);
    }
    Ok(DiffResult { root, warnings })
}
