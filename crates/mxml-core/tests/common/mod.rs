#![allow(dead_code)]

use mxml_core::model::{parse_score, Event, Part, Score};
use xmltree::{Element, XMLNode};

pub struct Note {
    step: String,
    octave: i32,
    duration: i32,
    voice: String,
    chord: bool,
    rest: bool,
    grace: bool,
    kind: String,
    extra: String,
}

impl Note {
    pub fn new(step: &str) -> Self {
        Self {
            step: step.into(),
            octave: 4,
            duration: 1,
            voice: "1".into(),
            chord: false,
            rest: false,
            grace: false,
            kind: "quarter".into(),
            extra: String::new(),
        }
    }
    pub fn octave(mut self, value: i32) -> Self {
        self.octave = value;
        self
    }
    pub fn duration(mut self, value: i32) -> Self {
        self.duration = value;
        self
    }
    pub fn voice(mut self, value: &str) -> Self {
        self.voice = value.into();
        self
    }
    pub fn kind(mut self, value: &str) -> Self {
        self.kind = value.into();
        self
    }
    pub fn extra(mut self, value: &str) -> Self {
        self.extra = value.into();
        self
    }
    pub fn chord(mut self) -> Self {
        self.chord = true;
        self
    }
    pub fn rest(mut self) -> Self {
        self.rest = true;
        self
    }
    pub fn grace(mut self) -> Self {
        self.grace = true;
        self
    }
    pub fn xml(self) -> String {
        format!(
            "<note>{}{}{}{}<voice>{}</voice><type>{}</type>{}</note>",
            if self.grace { "<grace/>" } else { "" },
            if self.chord { "<chord/>" } else { "" },
            if self.rest {
                "<rest/>".into()
            } else {
                format!(
                    "<pitch><step>{}</step><octave>{}</octave></pitch>",
                    self.step, self.octave
                )
            },
            if self.grace {
                String::new()
            } else {
                format!("<duration>{}</duration>", self.duration)
            },
            self.voice,
            self.kind,
            self.extra
        )
    }
}

pub fn n(step: &str) -> Note {
    Note::new(step)
}
pub fn backup(duration: i32) -> String {
    format!("<backup><duration>{duration}</duration></backup>")
}
pub fn forward(duration: i32) -> String {
    format!("<forward><duration>{duration}</duration></forward>")
}

pub fn score(parts: &[(&str, Vec<String>)]) -> Score {
    score_div(parts, 1)
}

pub fn score_div(parts: &[(&str, Vec<String>)], divisions: i32) -> Score {
    let definitions: String = parts
        .iter()
        .enumerate()
        .map(|(i, (name, _))| {
            format!(
                "<score-part id=\"P{}\"><part-name>{name}</part-name></score-part>",
                i + 1
            )
        })
        .collect();
    let contents: String = parts.iter().enumerate().map(|(i, (_, measures))| {
        let items: String = measures.iter().enumerate().map(|(index, body)| {
            let attributes = if index == 0 { format!("<attributes><divisions>{divisions}</divisions><key><fifths>0</fifths></key><time><beats>4</beats><beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef></attributes>") } else { String::new() };
            format!("<measure number=\"{}\">{attributes}{body}</measure>", index + 1)
        }).collect();
        format!("<part id=\"P{}\">{items}</part>", i + 1)
    }).collect();
    parse(&format!("<score-partwise version=\"4.0\"><work><work-title>Choir fixture</work-title></work><part-list>{definitions}</part-list>{contents}</score-partwise>"))
}

pub fn parse(xml: &str) -> Score {
    parse_score(element(xml)).unwrap()
}
pub fn element(xml: &str) -> Element {
    mxml_core::parse_xml(xml.as_bytes(), "test fixture").unwrap()
}
pub fn append(parent: &mut Element, node: Element) {
    parent.children.push(XMLNode::Element(node));
}
pub fn text(parent: &Element, path: &str) -> String {
    path.split('/')
        .try_fold(parent, |p, tag| p.get_child(tag))
        .and_then(Element::get_text)
        .map(|s| s.into_owned())
        .unwrap_or_default()
}
pub fn names(score: &Score) -> Vec<String> {
    score.parts.iter().map(Part::name).collect()
}
pub fn pitches(part: &Part, measure: usize) -> Vec<Vec<String>> {
    part.measures[measure]
        .events
        .iter()
        .filter(|e| !e.grace())
        .map(|e| {
            e.notes
                .iter()
                .map(|n| {
                    if n.get_child("rest").is_some() {
                        "rest".into()
                    } else {
                        text(n, "pitch/step")
                    }
                })
                .collect()
        })
        .collect()
}
pub fn event_notes(events: &[Event]) -> Vec<(&str, String, String)> {
    events
        .iter()
        .map(|e| {
            (
                e.voice.as_str(),
                text(&e.notes[0], "pitch/step"),
                text(&e.notes[0], "stem"),
            )
        })
        .collect()
}
pub fn elements(node: &Element) -> impl Iterator<Item = &Element> {
    node.children.iter().filter_map(XMLNode::as_element)
}
pub fn descendants(node: &Element) -> Vec<&Element> {
    let mut result = vec![node];
    for child in elements(node) {
        result.extend(descendants(child));
    }
    result
}
pub fn ids(node: &Element) -> Vec<String> {
    descendants(node)
        .iter()
        .filter_map(|e| e.attributes.get("id").cloned())
        .collect()
}
pub fn groups(ids: &[&[&str]]) -> Vec<Vec<String>> {
    ids.iter()
        .map(|g| g.iter().map(|id| (*id).to_owned()).collect())
        .collect()
}
