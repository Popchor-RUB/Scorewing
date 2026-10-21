"""Separate explicit voices and chord tones without guessing melodic lines."""

from __future__ import annotations

from collections import defaultdict
from copy import deepcopy
from fractions import Fraction

from lxml import etree as ET

from .model import (
    Event, Score, ScoreError, TimedElement, build_score, insert_note_child, render_measure,
    rest_event, set_child, slice_event, strip_position, voice_key,
)


def _pitch(note):
    pitch = note.find("pitch")
    if pitch is None:
        pitch = note.find("unpitched")
        if pitch is None:
            return None
        step = pitch.findtext("display-step", "C")
        octave = pitch.findtext("display-octave", "4")
    else:
        step, octave = pitch.findtext("step"), pitch.findtext("octave")
    try:
        return (int(octave) * 12 + {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}[step]
                + Fraction(pitch.findtext("alter", "0")), int(octave), "CDEFGAB".index(step))
    except (TypeError, ValueError, KeyError, ZeroDivisionError):
        raise ScoreError("Cannot split an invalid pitch") from None


def _notes(event, tone, mode, context):
    if mode == "voices" or len(event.notes) == 1:
        return event.notes
    if any(_pitch(note) is None for note in event.notes):
        raise ScoreError(f"{context}, position {event.start}: a chord cannot contain rests")
    notes = sorted(event.notes, key=_pitch, reverse=True)
    return notes[tone:tone + 1]


def _segments(measure, voice):
    events = [event for event in measure.events if not event.grace]
    boundaries = sorted({Fraction(0), measure.length, *(v for e in events for v in (e.start, e.end))})
    segments = []
    for start, end in zip(boundaries, boundaries[1:]):
        active = {event.voice: event for event in events if event.start <= start < event.end}
        event = active.get(voice)
        if event is None and len(active) == 1:
            event = next(iter(active.values()))
        if segments and segments[-1][2] is event:
            segments[-1] = (segments[-1][0], end, event)
        else:
            segments.append((start, end, event))
    return segments


def _lane_measure(measure, voice, tone, mode, assignments, lane):
    result = []
    for start, end, source in _segments(measure, voice):
        notes = _notes(source, tone, mode, measure.context) if source else []
        if not notes:
            result.append(rest_event(start, end - start))
            continue
        selected = Event(source.start, source.duration, deepcopy(notes), "1")
        event = slice_event(selected, start, end) if start != source.start or end != source.end else selected
        for original, copied in zip(notes, event.notes):
            assignments[id(original)].add(lane)
            if event is not selected:
                # A copied shared passage can end when another encoded voice enters.
                keep = {tie.get("type") for tie in original.findall("tie")
                        if tie.get("type") == "start" and end == source.end
                        or tie.get("type") == "stop" and start == source.start}
                for tie in copied.findall("tie") + copied.findall("notations/tied"):
                    if tie.get("type") not in keep:
                        tie.getparent().remove(tie)
            if mode == "chords" and len(source.notes) > 1 and start == source.start:
                _inherit_chord_marks(source.notes[0], copied)
        result.append(event)
    graces = [event for event in measure.events if event.grace]
    for source in graces:
        occupied = {event.voice for event in measure.events
                    if event.start <= source.start < event.end or event.grace and event.start == source.start}
        if source.voice != voice and len(occupied) != 1:
            continue
        notes = _notes(source, tone, mode, measure.context)
        if notes:
            copied = deepcopy(notes)
            if mode == "chords" and len(source.notes) > 1:
                for node in copied:
                    _inherit_chord_marks(source.notes[0], node)
            result.append(Event(source.start, Fraction(0), copied, "1"))
            for original in notes:
                assignments[id(original)].add(lane)
    result.sort(key=lambda event: (event.start, not event.grace))
    extras = []
    for extra in measure.extras:
        source_voice = extra.element.findtext("voice")
        if source_voice is not None:
            occupied = {event.voice for event in measure.events
                        if event.start <= extra.position < event.end}
            if source_voice != voice and occupied != {source_voice}:
                continue
            node = deepcopy(extra.element)
            set_child(node, "voice", "1")
            extras.append(TimedElement(extra.position, node, extra.divisions))
        else:
            extras.append(extra)
    return render_measure(measure, result, extras=extras)


def _inherit_chord_marks(primary, note):
    """Chord rhythm and shared text are normally written on its first tone only."""
    verses = {lyric.get("number", "1") for lyric in note.findall("lyric")}
    for lyric in primary.findall("lyric"):
        if lyric.get("number", "1") not in verses:
            insert_note_child(note, deepcopy(lyric))
    for tag in ("beam", "time-modification"):
        if note.find(tag) is None:
            for child in primary.findall(tag):
                insert_note_child(note, deepcopy(child))
    for tag in ("tuplet", "articulations"):
        if note.find(f"notations/{tag}") is None:
            for child in primary.findall(f"notations/{tag}"):
                notations = note.find("notations")
                if notations is None:
                    notations = ET.Element("notations")
                    insert_note_child(note, notations)
                notations.append(deepcopy(child))


def _check_relationships(part, assignments):
    ties, slurs = {}, {}
    for measure in part.measures:
        for event in sorted(measure.events, key=lambda event: event.start):
            for note in event.notes:
                lanes = assignments[id(note)]
                pitch = note.find("pitch")
                key = (event.voice, (pitch.findtext("step"), Fraction(pitch.findtext("alter", "0")),
                                     pitch.findtext("octave")) if pitch is not None else None)
                kinds = {tie.get("type") for tie in note.findall("tie") + note.findall("notations/tied")}
                if "stop" in kinds and key in ties:
                    if ties.pop(key) != lanes:
                        raise ScoreError(f"{measure.context}, position {event.start}: ambiguous tie assignment when splitting")
                if "start" in kinds:
                    ties[key] = lanes
                for slur in note.findall("notations/slur"):
                    key = (event.voice, slur.get("number", "1"))
                    kind = slur.get("type")
                    if kind == "stop" and key in slurs:
                        if slurs.pop(key) != lanes:
                            raise ScoreError(f"{measure.context}, position {event.start}: ambiguous slur assignment when splitting")
                    if kind == "start":
                        slurs[key] = lanes


def _copy_ids(definition, part, new_id, used):
    """MusicXML part and instrument references share IDs with their definitions."""
    mapping = {definition.get("id"): new_id}
    references = {"part", "instrument", "midi-instrument", "midi-device", "play",
                  "instrument-change", "instrument-link"}
    for root in (definition, part):
        for element in root.iter():
            old = element.get("id")
            if old is not None and element.tag not in references and old not in mapping:
                candidate = f"{new_id}_{len(mapping)}"
                while candidate in used:
                    candidate += "_"
                mapping[old] = candidate
                used.add(candidate)
    occurrences = defaultdict(int)
    for root in (definition, part):
        for element in root.iter():
            for key, value in list(element.attrib.items()):
                if key == "id" and value in mapping:
                    renamed = mapping[value]
                    if element.tag not in references:
                        occurrences[value] += 1
                        if occurrences[value] > 1:
                            renamed += f"_occurrence_{occurrences[value]}"
                            while renamed in used:
                                renamed += "_"
                            used.add(renamed)
                    element.set(key, renamed)
                elif key == "player" and value in mapping:
                    element.set(key, mapping[value])
                elif key.rsplit("}", 1)[-1] == "href" and value.startswith("#") and value[1:] in mapping:
                    element.set(key, "#" + mapping[value[1:]])
        strip_position(root)


def split_score(score: Score, mode: str = "chords") -> ET._Element:
    if mode not in ("chords", "voices"):
        raise ScoreError("Split mode must be 'chords' or 'voices'")
    used = {node.get("id") for node in score.root.iter() if node.get("id") is not None}
    replacements = {}
    for part in score.parts:
        widths = {}
        for measure in part.measures:
            for event in measure.events:
                widths[event.voice] = max(widths.get(event.voice, 1), len(event.notes) if mode == "chords" else 1)
        slots = [(voice, tone) for voice in sorted(widths, key=voice_key) for tone in range(widths[voice])]
        if len(slots) < 2:
            continue
        assignments = defaultdict(set)
        copies = [(part.definition, part.source)]
        for lane, (voice, tone) in enumerate(slots, 1):
            new_id = f"{part.id}_split_{lane}"
            while new_id in used:
                new_id += "_"
            used.add(new_id)
            definition = deepcopy(part.definition)
            definition.find("part-name").text = f"{part.name} {lane}"
            for tag in ("part-name-display", "part-abbreviation-display"):
                display = definition.find(tag)
                if display is not None:
                    definition.remove(display)
            abbreviation = definition.find("part-abbreviation")
            if abbreviation is not None:
                abbreviation.text = f"{abbreviation.text or part.name} {lane}"
            derived = ET.Element("part", dict(part.source.attrib))
            for measure in part.measures:
                derived.append(_lane_measure(measure, voice, tone, mode, assignments, lane))
            _copy_ids(definition, derived, new_id, used)
            copies.append((definition, derived))
        _check_relationships(part, assignments)
        replacements[part.id] = copies
    return build_score(score, replacements)
