from fractions import Fraction

import pytest
from lxml import etree as ET

from mxml.model import ScoreError, parse_score
from mxml.split import split_score
from tests.helpers import backup, note, score


def split(parts, mode="chords", **kwargs):
    return parse_score(split_score(score(parts, **kwargs), mode))


def pitches(part, measure=0):
    return [tuple(n.findtext("pitch/step", "rest") for n in e.notes)
            for e in part.measures[measure].events if not e.grace]


def test_late_divisi_scans_whole_score_and_retains_original_order():
    first = note("G")
    second = note("C") + note("G", chord=True)
    original = score([("Soprano", [first, second]), ("Alto", [note("E"), note("F")])])
    result = parse_score(split_score(original))
    assert [p.name for p in result.parts] == ["Soprano", "Soprano 1", "Soprano 2", "Alto"]
    assert ET.tostring(result.parts[0].source) == ET.tostring(original.parts[0].source)
    assert [pitches(p, 0) for p in result.parts[1:3]] == [[("G",)], [("G",)]]
    assert [pitches(p, 1) for p in result.parts[1:3]] == [[("G",)], [("C",)]]
    assert result.root.findtext("work/work-title") == "Choir fixture"


def test_monophonic_staves_unchanged_and_voices_mode_keeps_chords():
    body = note("C") + note("E", chord=True)
    original = score([("Soprano", [body])])
    assert ET.tostring(split_score(original, "voices")) == ET.tostring(original.root)
    mono = score([("Alto", [note("C"), note("D")])])
    assert ET.tostring(split_score(mono)) == ET.tostring(mono.root)
    result = split([("Soprano", [body + backup(1) + note("G", voice="2")])], "voices")
    assert pitches(result.parts[1]) == [("C", "E")]
    assert pitches(result.parts[2]) == [("G",)]
    assert {e.voice for p in result.parts[1:] for m in p.measures for e in m.events} == {"1"}


def test_voice_slots_numeric_then_lexical():
    body = ""
    for index, (voice, step) in enumerate([("z", "B"), ("10", "G"), ("2", "E"), ("a", "C")]):
        body += (backup(1) if index else "") + note(step, voice=voice)
    result = split([("Choir", [body])])
    assert [pitches(p) for p in result.parts[1:]] == [[("E",)], [("G",)], [("C",)], [("B",)]]


def test_partial_chords_fill_rests_and_single_notes_share_all_chord_lanes():
    body = (note("C") + note("E", chord=True) + note("G", chord=True)
            + note("D") + note("F", chord=True) + note("A") + note(rest=True))
    result = split([("Soprano", [body])])
    assert [pitches(p) for p in result.parts[1:]] == [
        [("G",), ("F",), ("A",), ("rest",)],
        [("E",), ("D",), ("A",), ("rest",)],
        [("C",), ("rest",), ("A",), ("rest",)],
    ]


def test_shared_explicit_voice_passages_and_explicit_rests():
    body = note("C") + note("D") + backup(1) + note(rest=True, voice="2") + note("E", voice="2")
    result = split([("Soprano", [body])])
    assert pitches(result.parts[1]) == [("C",), ("D",), ("E",)]
    assert pitches(result.parts[2]) == [("C",), ("rest",), ("E",)]


def test_mixed_explicit_voices_and_chords():
    body = note("C") + note("E", chord=True) + backup(1) + note("G", voice="2")
    result = split([("Soprano", [body])])
    assert [pitches(p) for p in result.parts[1:]] == [[("E",)], [("C",)], [("G",)]]


def test_pickups_forwards_and_silent_measures_retain_exact_length():
    body = '<forward><duration>1</duration></forward>' + note("C") + note("G", chord=True)
    result = split([("Soprano", [body, "", note("E")])], divisions=3)
    for part in result.parts[1:]:
        assert [m.length for m in part.measures] == [Fraction(2, 3), Fraction(4), Fraction(1, 3)]
        assert part.measures[0].events[0].notes[0].find("rest") is not None
        assert pitches(part, 1) == [("rest",)]


def test_changing_divisions_tuplets_and_graces_preserved():
    tuplet = '<time-modification><actual-notes>3</actual-notes><normal-notes>2</normal-notes></time-modification>'
    body = (note("D", grace=True) + note("F", grace=True, chord=True)
            + note("C", duration=2, note_type="quarter", extra=tuplet)
            + note("G", duration=2, chord=True, note_type="quarter", extra=tuplet)
            + '<attributes><divisions>6</divisions></attributes>' + note("E", duration=6))
    result = split([("Soprano", [body])], divisions=3)
    for part in result.parts[1:]:
        assert part.measures[0].length == Fraction(5, 3)
        assert part.measures[0].events[0].grace
        assert part.measures[0].events[1].notes[0].find("time-modification") is not None


def test_consistent_ties_preserved_and_ambiguous_ties_rejected():
    start = '<tie type="start"/><notations><tied type="start"/></notations>'
    stop = '<tie type="stop"/><notations><tied type="stop"/></notations>'
    first = note("C", extra=start) + note("G", chord=True)
    second = note("C", extra=stop) + note("G", chord=True)
    result = split([("Soprano", [first, second])])
    lower = result.parts[2]
    assert lower.measures[0].events[0].notes[0].find("tie").get("type") == "start"
    assert lower.measures[1].events[0].notes[0].find("tie").get("type") == "stop"
    with pytest.raises(ScoreError, match="Soprano.*measure 2.*position 0.*ambiguous tie"):
        split([("Soprano", [note("C", extra=start), second])])


def test_ambiguous_slurs_rejected():
    first = note("C", extra='<notations><slur type="start" number="1"/></notations>')
    second = (note("C", extra='<notations><slur type="stop" number="1"/></notations>')
              + note("G", chord=True))
    with pytest.raises(ScoreError, match="ambiguous slur"):
        split([("Soprano", [first, second])])


def test_identifiers_instruments_annotations_and_lyrics_preserved():
    original = score([("Soprano", [note("C", extra='<lyric number="1"><text>Ah</text></lyric>')
                                   + note("G", chord=True)])])
    definition = original.parts[0].definition
    ET.SubElement(definition, "part-name-display").append(ET.Element("display-text"))
    definition.find("part-name-display/display-text").text = "Old displayed name"
    ET.SubElement(definition, "score-instrument", id="I1")
    ET.SubElement(definition[-1], "instrument-name").text = "Choir"
    ET.SubElement(ET.SubElement(definition, "player", id="Singer"), "player-name").text = "Singer"
    for index, node in enumerate(original.parts[0].source.findall("measure/note"), 1):
        node.set("id", f"N{index}")
        node.set("default-x", "42")
        node.insert(2, ET.Element("instrument", id="I1"))
        ET.SubElement(ET.SubElement(node, "listen"), "assess", type="yes", player="Singer")
    result = split_score(original)
    definitions = result.findall("part-list/score-part")
    parts = result.findall("part")
    ids = [node.get("id") for node in result.iter()
           if node.get("id") and node.tag not in ("part", "instrument", "midi-instrument")]
    assert len(ids) == len(set(ids))
    for definition, part in zip(definitions[1:], parts[1:]):
        assert definition.find("part-name-display") is None
        assert part.find("measure/note/instrument").get("id") == definition.find("score-instrument").get("id")
        assert part.find("measure/note/listen/assess").get("player") == definition.find("player").get("id")
        assert all(node.get("default-x") is None for node in part.findall("measure/note"))
        assert part.findtext("measure/note/lyric/text") == "Ah"


def test_chord_rhythm_and_articulations_follow_each_derived_tone():
    marks = ('<time-modification><actual-notes>3</actual-notes><normal-notes>2</normal-notes></time-modification>'
             '<notations><articulations><staccato/></articulations><tuplet type="start"/></notations>'
             '<lyric number="1"><text>Sing</text></lyric>')
    body = note("C", duration=2, extra=marks) + note("G", duration=2, chord=True)
    original = score([("Soprano", [body])], divisions=3)
    before = ET.tostring(original.root)
    result = parse_score(split_score(original))
    assert ET.tostring(original.root) == before
    for part in result.parts[1:]:
        node = part.measures[0].events[0].notes[0]
        assert node.find("notations/articulations/staccato") is not None
        assert node.find("notations/tuplet").get("type") == "start"
        assert node.findtext("lyric/text") == "Sing"
        assert node.findtext("time-modification/actual-notes") == "3"


def test_every_polyphonic_part_is_split_and_unique_ids_do_not_collide():
    original = score([("Soprano", [note("C") + note("E", chord=True)]),
                      ("Alto", [note("D") + note("F", chord=True)])])
    original.parts[0].definition.find("part-name").set("id", "P1_split_1")
    result = parse_score(split_score(original))
    assert [part.name for part in result.parts] == ["Soprano", "Soprano 1", "Soprano 2", "Alto", "Alto 1", "Alto 2"]
    assert result.parts[1].id == "P1_split_1_"
    assert len({part.id for part in result.parts}) == 6


def test_mid_note_voice_entry_preserves_timing_without_dangling_added_ties():
    body = note("C", duration=2, note_type="half") + backup(1) + note("G", voice="2")
    result = split([("Soprano", [body])])
    assert [(e.start, e.duration) for e in result.parts[1].measures[0].events] == [(0, 2)]
    assert [(e.start, e.duration) for e in result.parts[2].measures[0].events] == [(0, 1), (1, 1)]
    assert result.parts[2].source.find("measure/note/tie") is None


def test_shared_note_reused_around_a_secondary_voice_has_unique_identifiers():
    body = note("C", duration=4, note_type="whole") + backup(3) + note("D", voice="2")
    original = score([("Soprano", [body])])
    primary = original.parts[0].measures[0].events[0].notes[0]
    primary.set("id", "original-note")
    primary.append(ET.Element("notations", id="original-notations"))
    result = split_score(original)
    for part in result.findall("part")[1:]:
        identifiers = [node.get("id") for node in part.iter() if node.get("id")]
        assert len(identifiers) == len(set(identifiers))
    parsed = parse_score(result)
    assert pitches(parsed.parts[2]) == [("C",), ("D",), ("C",)]
    assert [(e.start, e.duration) for e in parsed.parts[2].measures[0].events] == [(0, 1), (1, 1), (2, 2)]


def test_invalid_mode_rejected():
    with pytest.raises(ScoreError, match="mode"):
        split_score(score([("Soprano", [note()])]), "invalid")
