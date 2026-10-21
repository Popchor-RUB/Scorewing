"""Merge selected single-staff parts into ordered, stem-directed voices."""

from __future__ import annotations

from copy import deepcopy
from fractions import Fraction

from lxml import etree as ET

from .model import (Event, Score, ScoreError, TimedElement, build_score,
                    insert_note_child, render_measure, rest_event, set_child,
                    slice_event, voice_key)


def _xml(element):
    if element is None:
        return None
    copy = deepcopy(element)
    for node in copy.iter():
        node.tail = None
        if node.text is not None and not node.text.strip():
            node.text = None
    return ET.tostring(copy, method="c14n")


def _pitch(note):
    if note.find("rest") is not None:
        return ("rest",)
    pitch = note.find("pitch")
    if pitch is not None:
        return (pitch.findtext("step"), Fraction(pitch.findtext("alter", "0")),
                int(pitch.findtext("octave")))
    return ("unpitched", _xml(note.find("unpitched")))


def _tie_state(note):
    return tuple(tuple(sorted(t.get("type", "") for t in note.findall(path)))
                 for path in ("tie", "notations/tied"))


def _signature(event):
    return sorted([(_pitch(n), _tie_state(n))
                   for n in event.notes], key=repr)


def _select_groups(score, pairs):
    if not pairs:
        raise ScoreError("merge requires at least one --pair containing two or more staves")
    selected, groups = set(), []
    for pair in pairs:
        indices = []
        for selector in pair.split("+"):
            selector = selector.strip()
            if selector.isdecimal():
                index = int(selector) - 1
                matches = [index] if 0 <= index < len(score.parts) else []
            else:
                matches = [i for i, part in enumerate(score.parts) if part.name == selector]
            if not matches:
                raise ScoreError(f"Unknown stave selector {selector!r} in --pair {pair!r}")
            if len(matches) > 1:
                raise ScoreError(f"Ambiguous stave name {selector!r}; use its one-based score position")
            index = matches[0]
            if index in indices:
                raise ScoreError(f"Repeated stave {selector!r} in --pair {pair!r}")
            if index in selected:
                raise ScoreError(f"Overlapping merge groups: stave {selector!r} appears more than once")
            indices.append(index)
        if len(indices) < 2:
            raise ScoreError(f"--pair {pair!r} must contain two or more distinct staves")
        selected.update(indices)
        groups.append(indices)
    return groups


def _flatten(measure, voice):
    by_span = {}
    grace_counts = {}
    for event in measure.events:
        ordinal = 0
        if event.grace:
            counter = (event.start, event.voice)
            ordinal = grace_counts.get(counter, 0)
            grace_counts[counter] = ordinal + 1
        key = (event.start, event.duration, event.grace, ordinal)
        if key not in by_span:
            by_span[key] = Event(event.start, event.duration, deepcopy(event.notes), voice)
        else:
            by_span[key].notes.extend(deepcopy(event.notes))
    events = sorted(by_span.values(), key=lambda e: (e.start, not e.grace))
    end = Fraction(0)
    completed = []
    for event in events:
        if event.grace:
            completed.append(event)
            continue
        if event.start < end:
            raise ScoreError(f"{measure.context}, position {event.start}: incompatible overlapping "
                             "rhythmic spans cannot be flattened into one voice")
        if event.start > end:
            completed.append(rest_event(end, event.start - end, voice))
        sounded = [n for n in event.notes if n.find("rest") is None]
        event.notes = sounded or event.notes[:1]
        # Existing voices may encode the same unison more than once.
        unique = {}
        for note in event.notes:
            key = (_pitch(note), _tie_state(note))
            if key in unique:
                _combine_note(unique[key], note)
            else:
                unique[key] = note
        event.notes = list(unique.values())
        completed.append(event)
        end = event.end
    if end < measure.length:
        completed.append(rest_event(end, measure.length - end, voice))
    return completed


def _combine_note(target, source):
    """Keep distinct lyrics and compatible notations when unisons share one note."""
    for lyric in source.findall("lyric"):
        identity = deepcopy(lyric)
        identity.attrib.pop("number", None)
        identity.attrib.pop("name", None)
        existing = []
        for original in target.findall("lyric"):
            copy = deepcopy(original)
            copy.attrib.pop("number", None)
            copy.attrib.pop("name", None)
            existing.append(_xml(copy))
        if _xml(identity) not in existing:
            insert_note_child(target, deepcopy(lyric))
    for tag in ("notations",):
        other = source.find(tag)
        if other is None:
            continue
        own = target.find(tag)
        if own is None:
            # Notations precede lyrics in MusicXML.
            index = next((i for i, n in enumerate(target) if n.tag in ("lyric", "play", "listen")), len(target))
            target.insert(index, deepcopy(other))
            continue
        for annotation in other:
            same = [n for n in own if n.tag == annotation.tag]
            if annotation.tag in ("articulations", "ornaments", "technical") and same:
                for item in annotation:
                    if _xml(item) not in [_xml(n) for n in same[0]]:
                        same[0].append(deepcopy(item))
            elif _xml(annotation) not in [_xml(n) for n in same]:
                own.append(deepcopy(annotation))


def _lyric_lines(parts):
    mapping = {}
    for source, part in enumerate(parts):
        verses = {lyric.get("number", "1") for measure in part.measures for event in measure.events
                  for note in event.notes for lyric in note.findall("lyric")}
        for verse in sorted(verses, key=voice_key):
            mapping[source, verse] = str(len(mapping) + 1)
    return mapping


def _prepare(events, source, lyric_lines, relationships, default_instrument):
    for event in events:
        for note in event.notes:
            _renumber_relationships(note, source, relationships)
            if default_instrument is not None and note.find("instrument") is None:
                insert_note_child(note, ET.Element("instrument", id=default_instrument))
            for lyric in note.findall("lyric"):
                lyric.set("number", lyric_lines[source, lyric.get("number", "1")])
            stem = note.find("stem")
            stemless = note.find("rest") is not None or note.findtext("type") in ("whole", "breve", "long", "maxima")
            if stemless:
                if stem is not None:
                    note.remove(stem)
            elif stem is None or stem.text != "none":
                set_child(note, "stem", "up" if int(event.voice) % 2 else "down")


def _state_signature(tag, element):
    if tag == "transpose":
        try:
            values = tuple(Fraction(element.findtext(child, "0")) if element is not None else Fraction(0)
                           for child in ("diatonic", "chromatic", "octave-change"))
        except (ValueError, ZeroDivisionError):
            raise ScoreError("Invalid numeric transposition") from None
        double = element.find("double") if element is not None else None
        return values, double is not None, double.get("above", "no") if double is not None else None
    if element is None:
        return None
    value = deepcopy(element)
    for node in value.iter():
        # key-octave's number associates an octave with a particular key alteration.
        number = node.get("number") if node.tag == "key-octave" else None
        node.attrib.clear()
        if number is not None:
            node.set("number", number)
        if node.text is not None:
            node.text = node.text.strip()
    return _xml(value)


def _compatible(measures):
    reference = measures[0]
    for measure in measures[1:]:
        if (measure.length != reference.length or measure.source.get("number") != reference.source.get("number")
                or measure.source.get("implicit", "no") != reference.source.get("implicit", "no")):
            raise ScoreError(f"{measure.context}, position 0: incompatible measure boundaries with {reference.context}")
    for tag in ("time", "key", "transpose"):
        timelines = []
        for measure in measures:
            timeline = {Fraction(0): _state_signature(tag, measure.state.get(tag))}
            for extra in measure.extras:
                if extra.element.tag == "attributes" and extra.element.find(tag) is not None:
                    timeline[extra.position] = _state_signature(tag, extra.element.find(tag))
            timelines.append(timeline)
        boundaries = sorted(set().union(*(t.keys() for t in timelines)))
        for position in boundaries:
            values = [timeline[max(p for p in timeline if p <= position)] for timeline in timelines]
            if len(set(values)) != 1:
                conflict = next(i for i, value in enumerate(values) if value != values[0])
                raise ScoreError(f"{measures[conflict].context}, position {position}: incompatible {tag} with {reference.context}")


def _clef(parts):
    positions = []
    for part in parts:
        for measure in part.measures:
            for event in measure.events:
                for note in event.notes:
                    pitch = note.find("pitch")
                    if pitch is not None:
                        positions.append(int(pitch.findtext("octave")) * 7 + "CDEFGAB".index(pitch.findtext("step")))
    # Staff bounds are E4..F5 for treble and G2..A3 for bass.
    def burden(bottom, top):
        return sum(max(0, (bottom - p) // 2, (p - top) // 2) for p in positions)
    return ("G", "2") if burden(30, 38) <= burden(18, 26) else ("F", "4")


def _extras(measures, clef, first, relationships):
    result, seen = [], set()
    for source, measure in enumerate(measures):
        for extra in measure.extras:
            node = deepcopy(extra.element)
            _renumber_relationships(node, source, relationships)
            if node.tag == "attributes":
                if source:
                    continue
                for item in node.findall("clef"):
                    node.remove(item)
                if not len(node):
                    continue
            elif node.tag == "direction":
                for old in node.findall("voice"):
                    node.remove(old)
                identity = (extra.position, _xml(node))
                if identity in seen:
                    continue
                seen.add(identity)
                voice = ET.Element("voice")
                voice.text = str(source + 1)
                index = next((i for i, item in enumerate(node) if item.tag in ("staff", "sound", "listening")), len(node))
                node.insert(index, voice)
            else:
                identity = (extra.position, _xml(node))
                if identity in seen:
                    continue
                seen.add(identity)
            result.append(TimedElement(extra.position, node, extra.divisions))
    if first:
        attributes = ET.Element("attributes")
        node = ET.SubElement(attributes, "clef")
        ET.SubElement(node, "sign").text = clef[0]
        ET.SubElement(node, "line").text = clef[1]
        result.append(TimedElement(Fraction(0), attributes, measures[0].divisions))
    return result


_NUMBERED_RELATIONSHIPS = {"slur", "tuplet", "glissando", "slide", "wavy-line",
                           "hammer-on", "pull-off", "wedge", "octave-shift", "pedal", "bracket", "dashes"}


def _renumber_relationships(element, source, relationships):
    for node in element.iter():
        if node.tag in _NUMBERED_RELATIONSHIPS:
            key = (source, node.tag, node.get("number", "1"))
            node.set("number", relationships[key])


def _relationships(parts):
    mapping, counts = {}, {}
    for source, part in enumerate(parts):
        for node in part.source.iter():
            if node.tag in _NUMBERED_RELATIONSHIPS:
                key = (source, node.tag, node.get("number", "1"))
                if key not in mapping:
                    counts[node.tag] = counts.get(node.tag, 0) + 1
                    if counts[node.tag] > 16:
                        raise ScoreError(f"Part {part.name!r}: too many distinct {node.tag} relationships to merge safely")
                    mapping[key] = str(counts[node.tag])
    return mapping


_ID_REFERENCES = {"part", "instrument", "midi-device", "midi-instrument", "play",
                  "instrument-change", "instrument-link"}


def _unique_ids(output, definition, used):
    mapping = {}
    for node in [*definition.iter(), *output.iter()]:
        identifier = node.get("id")
        if identifier is not None and node.tag not in _ID_REFERENCES:
            suffix = 1
            candidate = identifier
            while candidate in used:
                candidate = f"{identifier}_merge{suffix}"
                suffix += 1
            node.set("id", candidate)
            used.add(candidate)
            mapping.setdefault(identifier, candidate)
    for node in [*definition.iter(), *output.iter()]:
        if node.tag in _ID_REFERENCES and node.get("id") in mapping:
            node.set("id", mapping[node.get("id")])
        for name, value in list(node.attrib.items()):
            if name == "player" and value in mapping:
                node.set(name, mapping[value])
            if name.rsplit("}", 1)[-1] == "href" and value.startswith("#") and value[1:] in mapping:
                node.set(name, "#" + mapping[value[1:]])


def _merge_events(streams, length):
    ordinary = [[event for event in stream if not event.grace] for stream in streams]
    grace = [[event for event in stream if event.grace] for stream in streams]
    boundaries = sorted({Fraction(0), length, *(point for stream in ordinary for event in stream for point in (event.start, event.end))})
    divergence = length
    for start, end in zip(boundaries, boundaries[1:]):
        active = [next((e for e in stream if e.start <= start < e.end), None) for stream in ordinary]
        if any(e is None for e in active) or any(
            (e.start, e.duration, _signature(e)) != (active[0].start, active[0].duration, _signature(active[0]))
            for e in active[1:]
        ):
            divergence = start
            break
    grace_positions = sorted({e.start for stream in grace for e in stream})
    for position in grace_positions:
        groups = [[_signature(e) for e in stream if e.start == position] for stream in grace]
        if any(g != groups[0] for g in groups[1:]):
            divergence = min(divergence, position)
    result = []
    # Keep the original rhythmic spelling when equal; only cut at a real divergence.
    shared = [e for e in ordinary[0] if e.start < divergence]
    for event in shared:
        end = min(event.end, divergence)
        merged = slice_event(event, event.start, end)
        merged.voice = "1"
        for stream in ordinary[1:]:
            for other in stream:
                if other.start < end and other.end > event.start:
                    for note in merged.notes:
                        for other_note in other.notes:
                            if _pitch(note) == _pitch(other_note):
                                _combine_note(note, other_note)
        result.append(merged)
    for stream in ordinary:
        for event in stream:
            if event.end > divergence:
                result.append(slice_event(event, max(divergence, event.start), event.end))
    for source, stream in enumerate(grace):
        for event in stream:
            if event.start >= divergence:
                result.append(event)
            elif source == 0:
                merged = deepcopy(event)
                for other_stream in grace[1:]:
                    candidates = [e for e in other_stream if e.start == event.start]
                    index = [e for e in stream if e.start == event.start].index(event)
                    for note, other_note in zip(merged.notes, candidates[index].notes):
                        _combine_note(note, other_note)
                result.append(merged)
    return sorted(result, key=lambda event: (event.start, not event.grace))


def merge_score(score: Score, pairs: list[str]) -> ET._Element:
    replacements = {}
    groups = _select_groups(score, pairs)
    replaced_nodes = {node for group in groups for index in group
                      for element in (score.parts[index].source, score.parts[index].definition)
                      for node in element.iter()}
    used_ids = {node.get("id") for node in score.root.iter() if node not in replaced_nodes and node.get("id") is not None}
    for indices in groups:
        parts = [score.parts[i] for i in indices]
        if len({len(p.measures) for p in parts}) != 1:
            raise ScoreError("Cannot merge parts with different numbers of measures: " + ", ".join(p.name for p in parts))
        anchor = score.parts[min(indices)]
        definition = deepcopy(anchor.definition)
        definition.find("part-name").text = " / ".join(p.name for p in parts)
        for tag in ("part-name-display", "part-abbreviation", "part-abbreviation-display"):
            for stale in definition.findall(tag):
                definition.remove(stale)
        for tag in ("score-instrument", "player", "midi-device", "midi-instrument"):
            for old in definition.findall(tag):
                definition.remove(old)
            for part in parts:
                for item in part.definition.findall(tag):
                    definition.append(deepcopy(item))
        output = ET.Element("part", dict(anchor.source.attrib))
        lines = _lyric_lines(parts)
        relationships = _relationships(parts)
        clef = _clef(parts)
        for index, measures in enumerate(zip(*(p.measures for p in parts))):
            _compatible(measures)
            streams = [_flatten(m, str(source + 1)) for source, m in enumerate(measures)]
            for source, stream in enumerate(streams):
                instrument = parts[source].definition.find("score-instrument")
                _prepare(stream, source, lines, relationships,
                         instrument.get("id") if instrument is not None else None)
            events = _merge_events(streams, measures[0].length)
            output.append(render_measure(measures[0], events,
                                         extras=_extras(measures, clef, index == 0, relationships)))
        _unique_ids(output, definition, used_ids)
        for part in parts:
            replacements[part.id] = []
        replacements[anchor.id] = [(definition, output)]
    return build_score(score, replacements)
