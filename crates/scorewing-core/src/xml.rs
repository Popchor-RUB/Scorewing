use xmltree::{Element, XMLNode};

pub fn children<'a>(element: &'a Element, name: &'a str) -> impl Iterator<Item = &'a Element> {
    element
        .children
        .iter()
        .filter_map(XMLNode::as_element)
        .filter(move |e| e.name == name)
}
pub fn children_mut<'a>(
    element: &'a mut Element,
    name: &'a str,
) -> impl Iterator<Item = &'a mut Element> {
    element
        .children
        .iter_mut()
        .filter_map(XMLNode::as_mut_element)
        .filter(move |e| e.name == name)
}
pub fn elements(element: &Element) -> impl Iterator<Item = &Element> {
    element.children.iter().filter_map(XMLNode::as_element)
}
pub fn child<'a>(element: &'a Element, name: &str) -> Option<&'a Element> {
    element.get_child(name)
}
pub fn child_mut<'a>(element: &'a mut Element, name: &str) -> Option<&'a mut Element> {
    element.get_mut_child(name)
}
pub fn text(element: &Element) -> String {
    element
        .get_text()
        .map(|t| t.into_owned())
        .unwrap_or_default()
}
pub fn child_text(element: &Element, name: &str) -> Option<String> {
    child(element, name).map(text)
}
pub fn set_text(element: &mut Element, value: impl Into<String>) {
    element
        .children
        .retain(|n| !matches!(n, XMLNode::Text(_) | XMLNode::CData(_)));
    element.children.push(XMLNode::Text(value.into()));
}
pub fn append(parent: &mut Element, element: Element) {
    parent.children.push(XMLNode::Element(element));
}
pub fn text_element(name: &str, value: impl Into<String>) -> Element {
    let mut element = Element::new(name);
    set_text(&mut element, value);
    element
}
pub fn remove_children(element: &mut Element, name: &str) {
    element
        .children
        .retain(|n| n.as_element().is_none_or(|e| e.name != name));
}
const NOTE_ORDER: &[&str] = &[
    "grace",
    "cue",
    "chord",
    "pitch",
    "unpitched",
    "rest",
    "duration",
    "tie",
    "instrument",
    "footnote",
    "level",
    "voice",
    "type",
    "dot",
    "accidental",
    "time-modification",
    "stem",
    "notehead",
    "notehead-text",
    "staff",
    "beam",
    "notations",
    "lyric",
    "play",
    "listen",
];
fn rank(name: &str) -> usize {
    NOTE_ORDER.iter().position(|n| *n == name).unwrap_or(100)
}
pub fn insert_note_child(parent: &mut Element, element: Element) {
    let index = parent
        .children
        .iter()
        .position(|n| {
            n.as_element()
                .is_some_and(|e| rank(&e.name) > rank(&element.name))
        })
        .unwrap_or(parent.children.len());
    parent.children.insert(index, XMLNode::Element(element));
}
pub fn set_child<'a>(
    parent: &'a mut Element,
    name: &str,
    value: impl Into<String>,
) -> &'a mut Element {
    if child(parent, name).is_none() {
        insert_note_child(parent, Element::new(name));
    }
    let node = child_mut(parent, name).unwrap();
    set_text(node, value);
    node
}
pub fn strip_position(element: &mut Element) {
    for key in ["default-x", "default-y", "relative-x", "relative-y"] {
        element.attributes.remove(key);
    }
    for child in element
        .children
        .iter_mut()
        .filter_map(XMLNode::as_mut_element)
    {
        strip_position(child);
    }
}
pub fn visit(element: &Element, visitor: &mut impl FnMut(&Element)) {
    visitor(element);
    for child in elements(element) {
        visit(child, visitor);
    }
}
pub fn visit_mut(element: &mut Element, visitor: &mut impl FnMut(&mut Element)) {
    visitor(element);
    for child in element
        .children
        .iter_mut()
        .filter_map(XMLNode::as_mut_element)
    {
        visit_mut(child, visitor);
    }
}
