//! Recover native score-wide marks independently of MuseScore export preferences.
use super::*;
use scorewing_core::xml::{elements, text_element, visit};
use scorewing_core::{frac, Fraction};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
enum Mark {
    Words(String),
    Rehearsal(String),
    Tempo(f64),
    Metronome(String, usize, String),
}

struct Located {
    measure: usize,
    at: Fraction,
    mark: Mark,
}

pub(super) fn preserve(source: &Element, root: &mut Element) -> Result<Vec<String>> {
    let mut warnings = Vec::new();
    let expected = native_marks(source, &mut warnings);
    let mut present = xml_marks(root, &expected);
    for (index, mark) in expected.into_iter().enumerate() {
        if present.iter().any(|p| same_mark(p, &mark)) {
            continue;
        }
        let Some(part) = children_mut(root, "part").next() else {
            break;
        };
        let mut divisions = frac(1);
        let mut target = None;
        for (i, measure) in children_mut(part, "measure").enumerate() {
            if let Some(value) = measure
                .get_child("attributes")
                .and_then(|a| child_text(a, "divisions"))
                .and_then(|v| rational(&v))
            {
                divisions = value;
            }
            if i == mark.measure {
                target = Some(measure);
                break;
            }
        }
        let label = format!(
            "{:?} at measure {}, beat {}",
            mark.mark,
            mark.measure + 1,
            &mark.at + frac(1)
        );
        let Some(measure) = target else {
            warnings.push(format!(
                "Could not preserve {label}: converted measure is missing."
            ));
            continue;
        };
        let offset = &mark.at * &divisions;
        if mark.at < frac(0) || offset.denom() != &1.into() {
            warnings.push(format!("Could not preserve {label}: converted divisions cannot represent its exact position."));
            continue;
        }
        let mut direction = Element::new("direction");
        direction
            .attributes
            .insert("id".into(), format!("scorewing-tempo-{index}"));
        let mut kind = Element::new("direction-type");
        match &mark.mark {
            Mark::Words(text) => kind
                .children
                .push(XMLNode::Element(text_element("words", text))),
            Mark::Rehearsal(text) => kind
                .children
                .push(XMLNode::Element(text_element("rehearsal", text))),
            Mark::Tempo(tempo) => {
                // MusicXML requires a direction-type even for a playback-only event.
                let words = text_element("words", "");
                kind.children.push(XMLNode::Element(words));
                let mut sound = Element::new("sound");
                sound.attributes.insert("tempo".into(), tempo.to_string());
                direction.children.push(XMLNode::Element(sound));
            }
            Mark::Metronome(unit, dots, value) => {
                let mut metronome = Element::new("metronome");
                metronome
                    .children
                    .push(XMLNode::Element(text_element("beat-unit", unit)));
                for _ in 0..*dots {
                    metronome
                        .children
                        .push(XMLNode::Element(Element::new("beat-unit-dot")));
                }
                metronome
                    .children
                    .push(XMLNode::Element(text_element("per-minute", value)));
                kind.children.push(XMLNode::Element(metronome));
            }
        }
        direction.children.insert(0, XMLNode::Element(kind));
        if mark.at != frac(0) {
            direction.children.insert(
                1,
                XMLNode::Element(text_element("offset", offset.numer().to_string())),
            );
        }
        let insertion = measure
            .children
            .iter()
            .position(|n| {
                n.as_element()
                    .is_some_and(|e| !matches!(e.name.as_str(), "print" | "attributes"))
            })
            .unwrap_or(measure.children.len());
        measure
            .children
            .insert(insertion, XMLNode::Element(direction));
        present.push(mark);
    }
    Ok(warnings)
}

fn same_mark(a: &Located, b: &Located) -> bool {
    a.measure == b.measure
        && a.at == b.at
        && match (&a.mark, &b.mark) {
            (Mark::Tempo(x), Mark::Tempo(y)) => (x - y).abs() < 0.00001,
            (Mark::Words(x), Mark::Words(y)) | (Mark::Rehearsal(x), Mark::Rehearsal(y)) => {
                normalized(x) == normalized(y)
            }
            (Mark::Metronome(a, x, n), Mark::Metronome(b, y, m)) => {
                a == b
                    && x == y
                    && match (n.parse::<f64>(), m.parse::<f64>()) {
                        (Ok(n), Ok(m)) => (n - m).abs() < 0.00001,
                        _ => n == m,
                    }
            }
            _ => a.mark == b.mark,
        }
}

fn normalized(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn native_marks(source: &Element, warnings: &mut Vec<String>) -> Vec<Located> {
    let mut found = Vec::new();
    for staff in children(source, "Staff") {
        let mut signature = frac(4);
        for (measure_index, measure) in children(staff, "Measure")
            .filter(|m| m.get_child("multiMeasureRest").is_none())
            .enumerate()
        {
            visit(measure, &mut |node| {
                if node.name == "TimeSig" {
                    if let (Some(n), Some(d)) = (
                        child_text(node, "sigN").and_then(|v| rational(&v)),
                        child_text(node, "sigD").and_then(|v| rational(&v)),
                    ) {
                        if d > frac(0) {
                            signature = n * frac(4) / d;
                        }
                    }
                }
            });
            let length = measure
                .attributes
                .get("len")
                .and_then(|v| rational(v))
                .map(|v| v * frac(4))
                .unwrap_or_else(|| signature.clone());
            let mut seen = 0usize;
            let mut total = 0usize;
            let mut gradual_seen = 0usize;
            let mut gradual_total = 0usize;
            visit(measure, &mut |n| {
                if matches!(n.name.as_str(), "Tempo" | "RehearsalMark") {
                    total += 1;
                }
                gradual_total += usize::from(gradual_tempo(n));
            });
            for voice in children(measure, "voice") {
                let mut position = Some(frac(0));
                let tuplets: BTreeMap<_, _> = children(voice, "Tuplet")
                    .filter_map(|node| {
                        // Nested tuplets require their ancestor ratios; leave their positions unverified.
                        if node.get_child("Tuplet").is_some() {
                            return None;
                        }
                        let id = node.attributes.get("id")?.clone();
                        let normal = rational(&child_text(node, "normalNotes")?)?;
                        let actual = rational(&child_text(node, "actualNotes")?)?;
                        (actual > frac(0)).then(|| (id, normal / actual))
                    })
                    .collect();
                for node in elements(voice) {
                    visit(node, &mut |change| {
                        if !gradual_tempo(change) {
                            return;
                        }
                        gradual_seen += 1;
                        let description = child_text(change, "beginText")
                            .or_else(|| child_text(change, "tempoChangeType"))
                            .unwrap_or_else(|| "gradual tempo change".into());
                        warnings.push(format!("GradualTempoChange '{description}' at measure {}, beat {}: displayed ritardando/accelerando and playback ramp preservation are unverified. Check this passage in the exported score.", measure_index + 1, position.as_ref().map(|p| (p + frac(1)).to_string()).unwrap_or_else(|| "unknown".into())));
                    });
                    if matches!(node.name.as_str(), "Tempo" | "RehearsalMark") {
                        seen += 1;
                        let text = node.get_child("text").map(native_text).unwrap_or_default();
                        let label = format!(
                            "{} '{}' at measure {}, beat {}",
                            node.name,
                            text,
                            measure_index + 1,
                            position
                                .as_ref()
                                .map(|p| (p + frac(1)).to_string())
                                .unwrap_or_else(|| "unknown".into())
                        );
                        let Some(at) = &position else {
                            warnings.push(format!("Preservation unverified for {label}: unsupported native rhythm before this mark."));
                            continue;
                        };
                        let mut push = |mark| {
                            found.push(Located {
                                measure: measure_index,
                                at: at.clone(),
                                mark,
                            })
                        };
                        if node.name == "RehearsalMark" {
                            push(Mark::Rehearsal(text));
                            continue;
                        }
                        if matches!(child_text(node, "play").as_deref(), Some("0" | "false")) {
                            // A display-only Tempo must not acquire a playback event.
                        } else if let Some(tempo) = child_text(node, "tempo")
                            .and_then(|v| v.parse::<f64>().ok())
                            .filter(|n| n.is_finite() && *n > 0.0)
                        {
                            push(Mark::Tempo(tempo * 60.0));
                        } else {
                            warnings.push(format!("Playback tempo preservation unverified for {label}: no supported numeric tempo."));
                        }
                        if !text.is_empty() {
                            if let Some((unit, dots, value, words)) = metronome_text(&text) {
                                if !words.trim().is_empty() {
                                    push(Mark::Words(words.trim().into()));
                                }
                                push(Mark::Metronome(unit, dots, value));
                            } else {
                                if text.contains(['♩', '𝅗𝅥', '𝅝', '♪', '𝅘𝅥𝅯', '�']) {
                                    warnings.push(format!("Metronome display preservation unverified for {label}: unsupported native symbol or metronome expression; available text was retained."));
                                }
                                push(Mark::Words(text));
                            }
                        }
                        if node.get_child("tempoChange").is_some() {
                            warnings.push(format!("Dynamic tempo interpretation unverified for {label}; numeric playback tempo and displayed text were retained."));
                        }
                    } else if matches!(node.name.as_str(), "Chord" | "Rest") {
                        if node.get_child("grace").is_some()
                            || elements(node).any(|e| {
                                matches!(
                                    e.name.as_str(),
                                    "acciaccatura"
                                        | "appoggiatura"
                                        | "grace4"
                                        | "grace16"
                                        | "grace32"
                                        | "grace8after"
                                        | "grace16after"
                                        | "grace32after"
                                )
                            })
                        {
                            continue;
                        }
                        position = position
                            .and_then(|at| duration(node, &length, &tuplets).map(|d| at + d));
                    } else if node.name == "location" {
                        position = position.and_then(|at| {
                            if child_text(node, "measures").is_some_and(|m| m != "0") {
                                return None;
                            }
                            child_text(node, "fractions")
                                .and_then(|v| rational(&v))
                                .map(|d| at + d * frac(4))
                        });
                    } else if node.name == "tick" {
                        position = None;
                    }
                }
            }
            if total != seen {
                warnings.push(format!("Tempo/rehearsal preservation unverified at measure {}, beat unknown: marks outside native voices are unsupported.", measure_index + 1));
            }
            if gradual_total != gradual_seen {
                warnings.push(format!("GradualTempoChange at measure {}, beat unknown: a gradual tempo change outside native voices has unverified display and playback ramp preservation. Check this passage in the exported score.", measure_index + 1));
            }
        }
    }
    found
}

fn gradual_tempo(node: &Element) -> bool {
    node.name == "GradualTempoChange"
        || (node.name == "Spanner"
            && node
                .attributes
                .get("type")
                .is_some_and(|kind| kind == "GradualTempoChange")
            && node.get_child("GradualTempoChange").is_none()
            && node.get_child("prev").is_none())
}

fn duration(
    node: &Element,
    length: &Fraction,
    tuplets: &BTreeMap<String, Fraction>,
) -> Option<Fraction> {
    let mut duration = if let Some(value) = child_text(node, "duration") {
        rational(&value)? * frac(4)
    } else {
        match child_text(node, "durationType")?.as_str() {
            "measure" => length.clone(),
            "long" => frac(16),
            "breve" => frac(8),
            "whole" => frac(4),
            "half" => frac(2),
            "quarter" => frac(1),
            "eighth" => Fraction::new(1.into(), 2.into()),
            "16th" => Fraction::new(1.into(), 4.into()),
            "32nd" => Fraction::new(1.into(), 8.into()),
            "64th" => Fraction::new(1.into(), 16.into()),
            "128th" => Fraction::new(1.into(), 32.into()),
            "256th" => Fraction::new(1.into(), 64.into()),
            _ => return None,
        }
    };
    if node.get_child("duration").is_none() {
        let dots = child_text(node, "dots")
            .unwrap_or_else(|| "0".into())
            .parse::<u32>()
            .ok()?;
        if dots > 8 {
            return None;
        }
        let mut extra = duration.clone();
        for _ in 0..dots {
            extra /= frac(2);
            duration += &extra;
        }
    }
    if let Some(id) = child_text(node, "Tuplet") {
        duration *= tuplets.get(&id)?;
    }
    Some(duration)
}

fn rational(value: &str) -> Option<Fraction> {
    let (n, d) = value.trim().split_once('/').unwrap_or((value.trim(), "1"));
    let n = n.parse::<i64>().ok()?;
    let d = d.parse::<i64>().ok()?;
    (d != 0).then(|| Fraction::new(n.into(), d.into()))
}

fn native_text(node: &Element) -> String {
    let mut text = String::new();
    for child in &node.children {
        match child {
            XMLNode::Text(s) | XMLNode::CData(s) => text.push_str(s),
            XMLNode::Element(e) if e.name == "sym" => {
                text.push_str(match e.get_text().as_deref() {
                    Some("metNoteQuarterUp") => "♩",
                    Some("metNoteHalfUp") => "𝅗𝅥",
                    Some("metNoteWhole") => "𝅝",
                    Some("metNote8thUp") => "♪",
                    Some("metNote16thUp") => "𝅘𝅥𝅯",
                    Some("metAugmentationDot") => ".",
                    _ => "�",
                })
            }
            XMLNode::Element(e) => text.push_str(&native_text(e)),
            _ => (),
        }
    }
    // MuseScore also stores rich text as escaped XML.
    if text.contains('<') {
        if let Ok(parsed) =
            scorewing_core::parse_xml(format!("<text>{text}</text>").as_bytes(), "tempo text")
        {
            return native_text(&parsed);
        }
    }
    text
}

fn metronome_text(text: &str) -> Option<(String, usize, String, String)> {
    let (left, value) = text.split_once('=')?;
    let value = value.trim();
    if value
        .parse::<f64>()
        .ok()
        .is_none_or(|v| !v.is_finite() || v <= 0.0)
    {
        return None;
    }
    let (index, symbol) = left
        .char_indices()
        .rev()
        .find(|(_, c)| matches!(c, '♩' | '𝅗𝅥' | '𝅝' | '♪' | '𝅘𝅥𝅯'))?;
    let dots = left[index + symbol.len_utf8()..]
        .chars()
        .filter(|c| *c == '.')
        .count();
    let unit = match symbol {
        '♩' => "quarter",
        '𝅗𝅥' => "half",
        '𝅝' => "whole",
        '♪' => "eighth",
        '𝅘𝅥𝅯' => "16th",
        _ => return None,
    };
    Some((unit.into(), dots, value.into(), left[..index].into()))
}

fn xml_marks(root: &mut Element, expected: &[Located]) -> Vec<Located> {
    let mut result = Vec::new();
    for part in children_mut(root, "part") {
        let mut divisions = frac(1);
        for (measure_index, measure) in children_mut(part, "measure").enumerate() {
            let mut position = frac(0);
            for node in measure
                .children
                .iter_mut()
                .filter_map(XMLNode::as_mut_element)
            {
                if node.name == "attributes" {
                    if let Some(d) = child_text(node, "divisions")
                        .and_then(|v| rational(&v))
                        .filter(|d| *d > frac(0))
                    {
                        divisions = d;
                    }
                } else if matches!(node.name.as_str(), "note" | "forward" | "backup") {
                    if node.get_child("chord").is_none() && node.get_child("grace").is_none() {
                        if let Some(d) = child_text(node, "duration").and_then(|v| rational(&v)) {
                            position +=
                                d / &divisions * frac(if node.name == "backup" { -1 } else { 1 });
                        }
                    }
                } else if node.name == "direction" || node.name == "sound" {
                    let at = &position
                        + child_text(node, "offset")
                            .and_then(|v| rational(&v))
                            .unwrap_or_else(|| frac(0))
                            / &divisions;
                    let mut tempo_words = false;
                    visit(node, &mut |element| {
                        let mark = match element.name.as_str() {
                            "words" => Some(Mark::Words(native_text(element))),
                            "rehearsal" => Some(Mark::Rehearsal(native_text(element))),
                            "sound" => element
                                .attributes
                                .get("tempo")
                                .and_then(|v| v.parse::<f64>().ok())
                                .map(Mark::Tempo),
                            "metronome" => child_text(element, "beat-unit")
                                .zip(child_text(element, "per-minute"))
                                .map(|(u, v)| {
                                    Mark::Metronome(
                                        u,
                                        children(element, "beat-unit-dot").count(),
                                        v,
                                    )
                                }),
                            _ => None,
                        };
                        if let Some(mark) = mark {
                            let located = Located {
                                measure: measure_index,
                                at: at.clone(),
                                mark,
                            };
                            tempo_words |= matches!(located.mark, Mark::Words(_))
                                && expected.iter().any(|e| same_mark(&located, e));
                            result.push(located);
                        }
                    });
                    if tempo_words {
                        node.attributes.insert(
                            "id".into(),
                            format!("scorewing-tempo-export-{}", result.len()),
                        );
                    }
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xml(text: &str) -> Element {
        scorewing_core::parse_xml(text.as_bytes(), "marks test").unwrap()
    }

    #[test]
    fn tracks_tuplets_dotted_rhythm_and_fraction_locations_exactly() {
        let source = xml(r#"<Score><Staff><Measure><voice>
          <Tuplet id="1"><normalNotes>2</normalNotes><actualNotes>3</actualNotes></Tuplet>
          <Chord><durationType>quarter</durationType><Tuplet>1</Tuplet></Chord>
          <Chord><durationType>quarter</durationType><Tuplet>1</Tuplet></Chord>
          <Chord><durationType>quarter</durationType><Tuplet>1</Tuplet></Chord>
          <Rest><durationType>eighth</durationType><dots>1</dots></Rest>
          <location><fractions>-1/8</fractions></location>
          <Tempo><play>0</play><text>Quietly</text></Tempo>
          <RehearsalMark><text>B</text></RehearsalMark>
          </voice></Measure></Staff></Score>"#);
        let mut warnings = Vec::new();
        let marks = native_marks(&source, &mut warnings);
        assert!(warnings.is_empty());
        assert_eq!(marks.len(), 2);
        assert!(marks
            .iter()
            .all(|m| m.at == Fraction::new(9.into(), 4.into())));
        assert_eq!(marks[0].mark, Mark::Words("Quietly".into()));
    }

    #[test]
    fn recognizes_existing_marks_and_annotates_only_tempo_words() {
        let source = xml(
            r#"<Score><Staff><Measure><voice><Rest><durationType>quarter</durationType></Rest><Tempo><tempo>2</tempo><text>Allegro &lt;sym&gt;metNoteQuarterUp&lt;/sym&gt; = 120</text></Tempo><RehearsalMark><text>A</text></RehearsalMark></voice></Measure></Staff></Score>"#,
        );
        let mut root = xml(
            r#"<score-partwise><part-list/><part id="P1"><measure><attributes><divisions>4</divisions></attributes>
          <direction><direction-type><words>cresc.</words></direction-type></direction>
          <note><rest/><duration>4</duration></note>
          <direction><direction-type><words>Allegro</words></direction-type></direction>
          <direction><direction-type><metronome><beat-unit>quarter</beat-unit><per-minute>120.0</per-minute></metronome></direction-type><sound tempo="120.000000"/></direction>
          <direction><direction-type><rehearsal>A</rehearsal></direction-type></direction>
          </measure></part></score-partwise>"#,
        );
        assert!(preserve(&source, &mut root).unwrap().is_empty());
        assert!(preserve(&source, &mut root).unwrap().is_empty());
        let measure = root
            .get_child("part")
            .unwrap()
            .get_child("measure")
            .unwrap();
        let directions: Vec<_> = children(measure, "direction").collect();
        assert_eq!(directions.len(), 4);
        assert!(!directions[0].attributes.contains_key("id"));
        assert!(directions[1].attributes["id"].starts_with("scorewing-tempo-"));
    }

    #[test]
    fn warns_instead_of_rounding_unrepresentable_mark_positions() {
        let source = xml(
            r#"<Score><Staff><Measure><voice><Tuplet id="1"><normalNotes>2</normalNotes><actualNotes>3</actualNotes></Tuplet><Chord><durationType>quarter</durationType><Tuplet>1</Tuplet></Chord><RehearsalMark><text>A</text></RehearsalMark></voice></Measure></Staff></Score>"#,
        );
        let mut root = xml("<score-partwise><part id=\"P1\"><measure><attributes><divisions>1</divisions></attributes></measure></part></score-partwise>");
        let warnings = preserve(&source, &mut root).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("measure 1, beat 5/3"));
        assert!(warnings[0].contains("exact position"));
    }

    #[test]
    fn warns_for_native_gradual_tempo_spanners_without_claiming_ramp_preservation() {
        let source = xml(r#"<Score><Staff>
          <Measure><voice><Rest><durationType>quarter</durationType></Rest>
            <Spanner type="GradualTempoChange"><GradualTempoChange><beginText>rit.</beginText><tempoChangeType>ritardando</tempoChangeType><tempoChangeFactor>0.5</tempoChangeFactor></GradualTempoChange><next><location><measures>1</measures></location></next></Spanner>
          </voice></Measure>
          <Measure><voice><Spanner type="GradualTempoChange"><prev><location><measures>-1</measures></location></prev></Spanner>
            <Chord><durationType>unsupported</durationType></Chord><GradualTempoChange><tempoChangeType>accelerando</tempoChangeType></GradualTempoChange>
          </voice></Measure>
          <Measure><GradualTempoChange><beginText>rall.</beginText></GradualTempoChange></Measure>
          </Staff></Score>"#);
        let mut warnings = Vec::new();
        assert!(native_marks(&source, &mut warnings).is_empty());
        assert_eq!(warnings.len(), 3, "{warnings:?}");
        assert!(warnings[0].contains("GradualTempoChange 'rit.' at measure 1, beat 2"));
        assert!(warnings[1].contains("accelerando' at measure 2, beat unknown"));
        assert!(warnings[2].contains("measure 3, beat unknown"));
        assert!(warnings
            .iter()
            .all(|w| w.contains("playback ramp") && w.contains("exported score")));
    }
}
