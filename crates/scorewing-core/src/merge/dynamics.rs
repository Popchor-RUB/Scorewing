//! Normalize dynamic engraving before relationship numbers and output voices are assigned.

use super::*;

struct Mark {
    source: usize,
    measure: usize,
    extra: TimedElement,
    position: Fraction,
    symbol: Element,
    identity: String,
    below: bool,
    duplicate: bool,
}

fn identity(symbol: &Element) -> String {
    let mut value = symbol.clone();
    walk_mut(&mut value, &mut |node| {
        node.attributes.retain(|key, _| {
            ["type", "niente", "print-object", "xml:lang"].contains(&key.as_str())
        });
        if node.attributes.get("niente").map(String::as_str) == Some("no") {
            node.attributes.remove("niente");
        }
    });
    canonical(&value)
}

fn extract(parts: &mut [Part]) -> Result<Vec<Mark>> {
    let mut marks = Vec::new();
    for (source, part) in parts.iter_mut().enumerate() {
        let mut origin = Fraction::zero();
        for (measure_index, measure) in part.measures.iter_mut().enumerate() {
            let mut retained = Vec::new();
            for extra in std::mem::take(&mut measure.extras) {
                let direction = &extra.element;
                if direction.name != "direction" {
                    retained.push(extra);
                    continue;
                }
                let symbols: Vec<_> = elements(direction)
                    .filter(|node| node.name == "direction-type")
                    .flat_map(elements)
                    .filter(|node| ["dynamics", "wedge"].contains(&node.name.as_str()))
                    .cloned()
                    .collect();
                if symbols.is_empty() {
                    retained.push(extra);
                    continue;
                }
                let offset = child_text(direction, "offset")
                    .map(|offset| numeric(&offset))
                    .transpose()?
                    .unwrap_or_default()
                    / &extra.divisions;
                let position = &origin + &extra.position + offset;
                let mut base = direction.clone();
                for kind in elements_mut(&mut base).filter(|node| node.name == "direction-type") {
                    remove_children(kind, &["dynamics", "wedge"]);
                }
                base.children.retain(|node| {
                    node.as_element().is_none_or(|kind| {
                        kind.name != "direction-type" || elements(kind).next().is_some()
                    })
                });
                let has_base = elements(&base).any(|node| node.name == "direction-type");
                if has_base {
                    retained.push(TimedElement {
                        element: base,
                        ..extra.clone()
                    });
                }
                for (index, symbol) in symbols.into_iter().enumerate() {
                    let mut node = direction.clone();
                    remove_children(&mut node, &["direction-type"]);
                    if has_base || index > 0 {
                        remove_children(&mut node, &["sound", "listening"]);
                    }
                    let mut kind = Element::new("direction-type");
                    append(&mut kind, symbol.clone());
                    node.children.insert(0, XMLNode::Element(kind));
                    marks.push(Mark {
                        source,
                        measure: measure_index,
                        extra: TimedElement {
                            element: node,
                            ..extra.clone()
                        },
                        position: position.clone(),
                        identity: identity(&symbol),
                        symbol,
                        below: false,
                        duplicate: false,
                    });
                }
            }
            measure.extras = retained;
            origin += &measure.length;
        }
    }
    Ok(marks)
}

fn levels(marks: &mut [Mark], stems: &[String]) {
    let mut order: Vec<_> = (0..marks.len()).collect();
    order.sort_by(|a, b| {
        marks[*a]
            .position
            .cmp(&marks[*b].position)
            .then_with(|| (stems[marks[*a].source] != "up").cmp(&(stems[marks[*b].source] != "up")))
    });
    let mut active: Vec<HashSet<String>> = vec![HashSet::new(); stems.len()];
    let mut seen = HashSet::new();
    let mut cursor = 0;
    while cursor < order.len() {
        let mut end = cursor + 1;
        while end < order.len() && marks[order[end]].position == marks[order[cursor]].position {
            end += 1;
        }
        let batch = &order[cursor..end];
        let mut updates: HashMap<usize, HashSet<String>> = HashMap::new();
        for index in batch {
            let mark = &marks[*index];
            if mark.symbol.name == "dynamics" {
                updates
                    .entry(mark.source)
                    .or_default()
                    .insert(mark.identity.clone());
            }
        }
        for (source, values) in updates {
            active[source] = values;
        }
        let upper_types: HashSet<_> = batch
            .iter()
            .filter(|index| stems[marks[**index].source] == "up")
            .map(|index| marks[*index].symbol.name.clone())
            .collect();
        for index in batch {
            let mark = &mut marks[*index];
            if stems[mark.source] == "down" {
                let own = if mark.symbol.name == "dynamics" {
                    HashSet::from([mark.identity.clone()])
                } else {
                    active[mark.source].clone()
                };
                mark.below = active.iter().enumerate().any(|(source, values)| {
                    !values.is_empty()
                        && (stems[source] == "up" && mark.symbol.name == "wedge"
                            || !own.is_empty() && *values != own)
                }) || upper_types.iter().any(|kind| *kind != mark.symbol.name);
            }
            if mark.symbol.name == "dynamics" {
                mark.duplicate = !seen.insert((mark.position.clone(), mark.identity.clone()));
            }
        }
        cursor = end;
    }
}

fn wedges(marks: &mut [Mark], stems: &[String]) -> Result<()> {
    let mut order: Vec<_> = (0..marks.len())
        .filter(|index| marks[*index].symbol.name == "wedge")
        .collect();
    order.sort_by(|a, b| marks[*a].position.cmp(&marks[*b].position));
    let mut open: HashMap<(usize, String), Vec<usize>> = HashMap::new();
    let mut spans: Vec<Vec<usize>> = Vec::new();
    for index in order {
        let mark = &marks[index];
        let key = (
            mark.source,
            mark.symbol
                .attributes
                .get("number")
                .cloned()
                .unwrap_or_else(|| "1".into()),
        );
        match mark.symbol.attributes.get("type").map(String::as_str) {
            Some("crescendo" | "diminuendo") => {
                if open.insert(key, vec![index]).is_some() {
                    return Err(ScoreError::new(
                        "Overlapping hairpins reuse the same source relationship number",
                    ));
                }
            }
            Some("continue") => {
                if let Some(span) = open.get_mut(&key) {
                    span.push(index);
                }
            }
            Some("stop") => {
                if let Some(mut span) = open.remove(&key) {
                    span.push(index);
                    spans.push(span);
                }
            }
            _ => {}
        }
    }
    // Unclosed hairpins are preserved, but cannot safely be treated as duplicates.
    spans.extend(open.into_values());
    spans.sort_by_key(|span| stems[marks[span[0]].source] != "up");
    let signature = |span: &[usize]| -> Vec<_> {
        // Continuation markers describe line breaks, not a different musical span.
        [span[0], *span.last().unwrap()]
            .into_iter()
            .map(|index| (marks[index].position.clone(), marks[index].identity.clone()))
            .collect()
    };
    let signatures: Vec<_> = spans.iter().map(|span| signature(span)).collect();
    let mut seen = HashSet::new();
    for (index, span) in spans.iter().enumerate() {
        let first = &marks[span[0]];
        let last = &marks[*span.last().unwrap()];
        let closed = last.symbol.attributes.get("type").map(String::as_str) == Some("stop");
        let duplicate = closed && !seen.insert(signatures[index].clone());
        let below = stems[first.source] == "down"
            && (first.below
                || spans.iter().enumerate().any(|(other_index, other)| {
                    let start = &marks[other[0]];
                    let end = &marks[*other.last().unwrap()];
                    (first.position == start.position
                        || first.position < end.position && start.position < last.position)
                        && signatures[index] != signatures[other_index]
                }));
        for mark in span {
            marks[*mark].duplicate = duplicate;
            marks[*mark].below = below;
        }
    }
    for span in &spans {
        let first = &marks[span[0]];
        if stems[first.source] != "up" {
            continue;
        }
        let start = first.position.clone();
        let last = &marks[*span.last().unwrap()];
        let end = (last.symbol.attributes.get("type").map(String::as_str) == Some("stop"))
            .then(|| last.position.clone());
        for mark in &mut *marks {
            if mark.symbol.name == "dynamics"
                && stems[mark.source] == "down"
                && mark.position >= start
                && end.as_ref().is_none_or(|end| mark.position < *end)
            {
                mark.below = true;
            }
        }
    }
    Ok(())
}

pub(super) fn normalize(parts: &mut [Part], stems: &[String]) -> Result<()> {
    for part in &mut *parts {
        for measure in &mut part.measures {
            for event in &mut measure.events {
                for note in &mut event.notes {
                    for notations in elements_mut(note).filter(|node| node.name == "notations") {
                        let symbols: Vec<_> = elements(notations)
                            .filter(|node| node.name == "dynamics")
                            .cloned()
                            .collect();
                        remove_children(notations, &["dynamics"]);
                        for symbol in symbols {
                            let mut kind = Element::new("direction-type");
                            append(&mut kind, symbol);
                            let mut direction = Element::new("direction");
                            append(&mut direction, kind);
                            measure.extras.push(TimedElement {
                                position: event.start.clone(),
                                element: direction,
                                divisions: measure.divisions.clone(),
                            });
                        }
                    }
                    note.children.retain(|node| {
                        node.as_element().is_none_or(|node| {
                            node.name != "notations" || elements(node).next().is_some()
                        })
                    });
                }
            }
        }
    }
    let mut marks = extract(parts)?;
    levels(&mut marks, stems);
    wedges(&mut marks, stems)?;
    for mut mark in marks {
        if mark.duplicate {
            if mark.extra.element.get_child("sound").is_none()
                && mark.extra.element.get_child("listening").is_none()
            {
                continue;
            }
            // A direction requires a direction-type even when only playback data remains.
            remove_children(&mut mark.extra.element, &["direction-type"]);
            let mut kind = Element::new("direction-type");
            append(&mut kind, Element::new("words"));
            mark.extra
                .element
                .children
                .insert(0, XMLNode::Element(kind));
        }
        mark.extra.element.attributes.insert(
            "placement".into(),
            if mark.below { "below" } else { "above" }.into(),
        );
        parts[mark.source].measures[mark.measure]
            .extras
            .push(mark.extra);
    }
    for part in parts {
        for measure in &mut part.measures {
            measure.extras.sort_by(|a, b| a.position.cmp(&b.position));
        }
    }
    Ok(())
}
