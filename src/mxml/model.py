"""Exact musical timing with the original XML attached to every event."""

from __future__ import annotations

from copy import deepcopy
from dataclasses import dataclass
from fractions import Fraction
from math import lcm

from lxml import etree as ET


class ScoreError(ValueError):
    """An unsupported or inconsistent score, reported without a traceback."""


@dataclass
class Event:
    start: Fraction
    duration: Fraction
    notes: list[ET._Element]
    voice: str = "1"

    @property
    def end(self) -> Fraction:
        return self.start + self.duration

    @property
    def grace(self) -> bool:
        return self.notes[0].find("grace") is not None


@dataclass
class TimedElement:
    position: Fraction
    element: ET._Element
    divisions: Fraction


@dataclass
class Measure:
    source: ET._Element
    events: list[Event]
    extras: list[TimedElement]
    length: Fraction
    divisions: Fraction
    state: dict[str, ET._Element]
    context: str


@dataclass
class Part:
    source: ET._Element
    definition: ET._Element
    measures: list[Measure]

    @property
    def id(self) -> str:
        return self.source.get("id", "")

    @property
    def name(self) -> str:
        return self.definition.findtext("part-name", self.id)


@dataclass
class Score:
    root: ET._Element
    parts: list[Part]


def voice_key(voice: str) -> tuple:
    return (0, int(voice), voice) if voice.isdecimal() else (1, voice)


def quantity(text: str | None, context: str, *, positive: bool = False) -> Fraction:
    try:
        if "/" in (text or ""):
            raise ValueError
        result = Fraction(text or "")
    except (ValueError, ZeroDivisionError):
        raise ScoreError(f"{context}: expected a finite number, got {text!r}") from None
    if result < 0 or positive and result == 0:
        raise ScoreError(f"{context}: value must be {'positive' if positive else 'nonnegative'}")
    return result


def parse_score(root: ET._Element) -> Score:
    if root.tag != "score-partwise":
        raise ScoreError("Only score-partwise MusicXML is supported; export score-timewise as score-partwise first")
    definitions = root.findall("part-list/score-part")
    sources = root.findall("part")
    ids = [p.get("id") for p in definitions]
    if not ids or None in ids or len(set(ids)) != len(ids):
        raise ScoreError("part-list must contain distinct part IDs")
    if [p.get("id") for p in sources] != ids:
        raise ScoreError("part-list and score parts must have the same IDs in the same order")
    parts = []
    for source, definition in zip(sources, definitions):
        name = definition.findtext("part-name", source.get("id"))
        for node in source.iter():
            multiple = node.tag in {"staves", "staff"} and (node.text or "").strip() != "1"
            numbered = node.tag in {"clef", "key", "time", "staff-details", "transpose", "measure-style"} and node.get("number", "1") != "1"
            if multiple or numbered:
                raise ScoreError(f"Part {name!r}: multi-staff instruments are outside the supported scope")
        divisions = Fraction(1)
        state: dict[str, ET._Element] = {}
        measures = []
        for source_measure in source.findall("measure"):
            context = f"Part {name!r}, measure {source_measure.get('number', '?')}"
            measure, divisions = _parse_measure(source_measure, divisions, state, context)
            measures.append(measure)
        if not measures:
            raise ScoreError(f"Part {name!r}: no measures")
        parts.append(Part(source, definition, measures))
    return Score(root, parts)


def _parse_measure(source, divisions, state, context):
    cursor = Fraction(0)
    extent = Fraction(0)
    events: list[Event] = []
    extras = []
    previous = None
    start_state = deepcopy(state)
    for node in source:
        if not isinstance(node.tag, str):
            extras.append(TimedElement(cursor, node, divisions))
            continue
        location = f"{context}, position {cursor}"
        if node.tag == "attributes":
            if node.find("divisions") is not None:
                divisions = quantity(node.findtext("divisions"), location + " divisions", positive=True)
            for child in node:
                if child.tag != "divisions":
                    state[child.tag] = deepcopy(child)
            if cursor == 0:
                start_state = deepcopy(state)
            extras.append(TimedElement(cursor, node, divisions))
        elif node.tag in ("backup", "forward"):
            duration = quantity(node.findtext("duration"), location, positive=True) / divisions
            cursor += -duration if node.tag == "backup" else duration
            if cursor < 0:
                raise ScoreError(f"{location}: backup crosses the beginning of the measure")
            extent = max(extent, cursor)
            previous = None
        elif node.tag == "note":
            grace = node.find("grace") is not None
            duration = Fraction(0) if grace else quantity(node.findtext("duration"), location, positive=True) / divisions
            voice = node.findtext("voice", "1")
            if node.find("chord") is not None:
                if previous is None or previous.voice != voice or previous.duration != duration or previous.grace != grace:
                    raise ScoreError(f"{location}: chord tones must share a voice, onset, and duration")
                previous.notes.append(node)
            else:
                previous = Event(cursor, duration, [node], voice)
                events.append(previous)
                cursor += duration
            extent = max(extent, cursor)
        else:
            extras.append(TimedElement(cursor, node, divisions))
    for voice in {event.voice for event in events}:
        end = Fraction(0)
        for event in sorted((e for e in events if e.voice == voice and not e.grace), key=lambda e: e.start):
            if event.start < end:
                raise ScoreError(f"{context}, position {event.start}: overlapping notes in voice {voice}; use chord markers")
            end = event.end
    if extent == 0:
        time = state.get("time")
        if time is not None:
            beats = time.findall("beats")
            types = time.findall("beat-type")
            if len(beats) != len(types):
                raise ScoreError(f"{context}: invalid time signature")
            try:
                extent = sum((sum(Fraction(b) for b in (beat.text or "").split("+")) * 4 / int(kind.text)
                              for beat, kind in zip(beats, types)), Fraction(0))
            except (ValueError, ZeroDivisionError, TypeError):
                raise ScoreError(f"{context}: invalid time signature") from None
    return Measure(source, events, extras, extent, divisions, start_state, context), divisions


NOTE_ORDER = {tag: i for i, tag in enumerate((
    "grace", "cue", "chord", "pitch", "unpitched", "rest", "duration", "tie", "instrument", "footnote", "level",
    "voice", "type", "dot", "accidental", "time-modification", "stem", "notehead", "notehead-text", "staff", "beam",
    "notations", "lyric", "play", "listen",
))}


def insert_note_child(note: ET._Element, child: ET._Element) -> None:
    rank = NOTE_ORDER.get(child.tag, 100)
    for index, existing in enumerate(note):
        if NOTE_ORDER.get(existing.tag, 100) > rank:
            note.insert(index, child)
            return
    note.append(child)


def set_child(parent: ET._Element, tag: str, text: str) -> ET._Element:
    child = parent.find(tag)
    if child is None:
        child = ET.Element(tag)
        insert_note_child(parent, child)
    child.text = text
    return child


def strip_position(element: ET._Element) -> None:
    for node in element.iter():
        for key in ("default-x", "default-y", "relative-x", "relative-y"):
            node.attrib.pop(key, None)


def rest_event(start: Fraction, duration: Fraction, voice: str = "1") -> Event:
    note = ET.Element("note")
    ET.SubElement(note, "rest")
    set_written_duration(note, duration)
    return Event(start, duration, [note], voice)


def set_written_duration(note: ET._Element, duration: Fraction) -> None:
    """Set exact written rhythm; retain the original tuplet ratio when it fits."""
    modification = note.find("time-modification")
    ratio = Fraction(1)
    if modification is not None:
        try:
            ratio = Fraction(int(modification.findtext("actual-notes")), int(modification.findtext("normal-notes")))
        except (TypeError, ValueError, ZeroDivisionError):
            raise ScoreError("Invalid tuplet time-modification") from None
    types = [(Fraction(32, 2**i), name) for i, name in enumerate(
        ("maxima", "long", "breve", "whole", "half", "quarter", "eighth", "16th", "32nd", "64th", "128th", "256th", "512th", "1024th"))]
    for base, name in types:
        for dots in range(4):
            if base * (2 - Fraction(1, 2**dots)) == duration * ratio:
                set_child(note, "type", name)
                for dot in note.findall("dot"):
                    note.remove(dot)
                for _ in range(dots):
                    insert_note_child(note, ET.Element("dot"))
                return
    # An irregular cut still needs an exact representable duration, including inside tuplets.
    set_child(note, "type", "quarter")
    for node in note.findall("dot") + note.findall("time-modification"):
        note.remove(node)
    modification = ET.Element("time-modification")
    ET.SubElement(modification, "actual-notes").text = str(duration.denominator)
    ET.SubElement(modification, "normal-notes").text = str(duration.numerator)
    insert_note_child(note, modification)


def slice_event(event: Event, start: Fraction, end: Fraction) -> Event:
    if not event.start <= start < end <= event.end:
        raise ScoreError("Invalid event slice")
    notes = deepcopy(event.notes)
    if start == event.start and end == event.end:
        return Event(start, end - start, notes, event.voice)
    for note in notes:
        if start != event.start or end != event.end:
            set_written_duration(note, end - start)
            for beam in note.findall("beam"):
                note.remove(beam)
        original_ties = {tie.get("type") for tie in note.findall("tie")}
        tie_templates = {tie.get("type"): deepcopy(tie) for tie in note.findall("tie")}
        tied_templates = {tie.get("type"): deepcopy(tie) for tie in note.findall("notations/tied")}
        for tie in note.findall("tie"):
            note.remove(tie)
        notations = note.find("notations")
        if notations is not None:
            for child in list(notations):
                if child.tag == "tied" or child.tag == "slur" and start != event.start:
                    notations.remove(child)
                elif start != event.start and child.tag not in ("slur", "tuplet"):
                    notations.remove(child)
        if start != event.start:
            for lyric in note.findall("lyric"):
                note.remove(lyric)
        if note.find("rest") is None:
            ties = []
            if start > event.start or "stop" in original_ties:
                ties.append("stop")
            if end < event.end or "start" in original_ties:
                ties.append("start")
            for kind in ties:
                original_endpoint = kind == "stop" and start == event.start or kind == "start" and end == event.end
                insert_note_child(note, deepcopy(tie_templates[kind]) if original_endpoint and kind in tie_templates
                                  else ET.Element("tie", type=kind))
                if notations is None:
                    notations = ET.Element("notations")
                    insert_note_child(note, notations)
                notations.append(deepcopy(tied_templates[kind]) if original_endpoint and kind in tied_templates
                                 else ET.Element("tied", type=kind))
    return Event(start, end - start, notes, event.voice)


def render_measure(measure: Measure, events: list[Event], *, extras: list[TimedElement] | None = None,
                   length: Fraction | None = None, divisions: int | None = None) -> ET._Element:
    """Serialize voice streams with exact backups and forwards, retaining timed metadata."""
    extras = measure.extras if extras is None else extras
    length = measure.length if length is None else length
    denominators = [Fraction(measure.divisions).numerator, *(v.denominator for e in events for v in (e.start, e.duration)),
                    length.denominator, *(x.position.denominator for x in extras)]
    for extra in extras:
        for offset in extra.element.findall("offset"):
            denominators.append(Fraction(offset.text or "0").__truediv__(extra.divisions).denominator)
    divisions = lcm(*denominators, divisions or 1)
    result = ET.Element("measure", dict(measure.source.attrib))
    result.attrib.pop("width", None)
    attributes = ET.SubElement(result, "attributes")
    ET.SubElement(attributes, "divisions").text = str(divisions)
    cursor = Fraction(0)

    def move(position, voice=None):
        nonlocal cursor
        delta = position - cursor
        if delta:
            node = ET.SubElement(result, "backup" if delta < 0 else "forward")
            ET.SubElement(node, "duration").text = str(int(abs(delta) * divisions))
            if delta > 0 and voice is not None:
                ET.SubElement(node, "voice").text = voice
        cursor = position

    def emit_extra(extra):
        move(extra.position)
        node = deepcopy(extra.element)
        strip_position(node)
        if node.tag == "attributes":
            for child in node.findall("divisions"):
                node.remove(child)
            if not len(node):
                return
        for offset in node.findall("offset"):
            offset.text = str(int(Fraction(offset.text or "0") * divisions / extra.divisions))
        result.append(node)

    right = [x for x in extras if x.element.tag == "barline" and x.element.get("location", "right") == "right"]
    pending = sorted((x for x in extras if x not in right), key=lambda x: x.position)
    voices = sorted({event.voice for event in events}, key=voice_key)
    for track, voice in enumerate(voices):
        move(Fraction(0))
        for event in sorted((e for e in events if e.voice == voice), key=lambda e: e.start):
            if track == 0:
                while pending and pending[0].position <= event.start:
                    emit_extra(pending.pop(0))
            if event.start < 0 or event.end > length:
                raise ScoreError(f"{measure.context}, position {event.start}: event outside measure")
            move(event.start, voice)
            for index, original in enumerate(event.notes):
                note = deepcopy(original)
                strip_position(note)
                for chord in note.findall("chord"):
                    note.remove(chord)
                if index:
                    insert_note_child(note, ET.Element("chord"))
                if not event.grace:
                    set_child(note, "duration", str(int(event.duration * divisions)))
                set_child(note, "voice", voice)
                for staff in note.findall("staff"):
                    note.remove(staff)
                result.append(note)
            if not event.grace:
                cursor = event.end
        if track == 0:
            while pending:
                emit_extra(pending.pop(0))
        move(length, voice)
    if not voices:
        while pending:
            emit_extra(pending.pop(0))
        move(length)
    for extra in right:
        result.append(deepcopy(extra.element))
    return result


def build_score(score: Score, replacements: dict[str, list[tuple[ET._Element, ET._Element]]]) -> ET._Element:
    """Replace part-list entries and parts together, preserving metadata and part groups."""
    root = deepcopy(score.root)
    part_list = root.find("part-list")
    for definition in list(part_list):
        if definition.tag == "score-part" and definition.get("id") in replacements:
            index = part_list.index(definition)
            part_list.remove(definition)
            for offset, (new_definition, _) in enumerate(replacements[definition.get("id")]):
                part_list.insert(index + offset, deepcopy(new_definition))
    for part in list(root.findall("part")):
        if part.get("id") in replacements:
            index = root.index(part)
            root.remove(part)
            for offset, (_, new_part) in enumerate(replacements[part.get("id")]):
                root.insert(index + offset, deepcopy(new_part))
    open_groups = {}
    count = 0
    for child in list(part_list):
        if child.tag == "score-part":
            count += 1
        elif child.tag == "part-group":
            number = child.get("number", "1")
            if child.get("type") == "start":
                open_groups[number] = (child, count)
            elif child.get("type") == "stop" and number in open_groups:
                start, first = open_groups.pop(number)
                if first == count:
                    part_list.remove(start)
                    part_list.remove(child)
    return root
