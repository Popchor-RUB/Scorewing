from fractions import Fraction

import pytest
from lxml import etree as ET

from mxml.io import validate_score
from mxml.merge import merge_score
from mxml.model import ScoreError, parse_score

from .helpers import backup, note, score


def merged(parts, pairs=None, **kwargs):
    root = merge_score(score(parts, **kwargs), pairs or ["1+2"])
    validate_score(root)
    return parse_score(root)


def test_group_order_selectors_and_unselected_parts():
    result = merged([("Soprano", [note("C")]), ("Alto", [note("E")]),
                     ("Tenor", [note("G")]), ("Bass", [note("C", 3)])],
                    ["Alto+1", "3+Bass"])
    assert [p.name for p in result.parts] == ["Alto / Soprano", "Tenor / Bass"]
    events = result.parts[0].measures[0].events
    assert [(e.voice, e.notes[0].findtext("pitch/step"), e.notes[0].findtext("stem"))
            for e in events] == [("1", "E", "up"), ("2", "C", "down")]
    result = merged([("A", [note("C")]), ("Keep", [note("D")]), ("B", [note("E")])], ["3+1"])
    assert [p.name for p in result.parts] == ["B / A", "Keep"]


@pytest.mark.parametrize("pairs,match", [([], "requires"), (["1"], "two or more"),
    (["1+1"], "Repeated"), (["1+2", "2+3"], "Overlapping"), (["1+4"], "Unknown"),
    (["Missing+2"], "Unknown"), (["Same+3"], "Ambiguous")])
def test_bad_groups(pairs, match):
    original = score([("Same", [note()]), ("Same", [note()]), ("Other", [note()])])
    with pytest.raises(ScoreError, match=match):
        merge_score(original, pairs)


def test_unison_prefix_divergence_and_barline_reset():
    first = note("C") + note("D") + note("E") + note("F")
    second = note("C") + note("E") + note("E") + note("F")
    result = merged([("S", [first, first]), ("A", [second, first])])
    one, two = result.parts[0].measures
    assert [(e.start, e.voice) for e in one.events] == [
        (0, "1"), (1, "1"), (2, "1"), (3, "1"), (1, "2"), (2, "2"), (3, "2")]
    assert {e.voice for e in two.events} == {"1"}


def test_three_sources_stems_and_matching_rests():
    result = merged([("S", [note(rest=True) + note("C")]),
                     ("A", [note(rest=True) + note("D")]),
                     ("T", [note(rest=True) + note("E")])], ["1+2+3"])
    events = result.parts[0].measures[0].events
    assert len(events) == 4
    assert events[0].notes[0].find("rest") is not None
    assert [e.notes[0].findtext("stem") for e in events[1:]] == ["up", "down", "up"]


def test_flatten_chords_and_reject_incompatible_overlap():
    chord = note("C", duration=2, note_type="half") + backup(2) + note("E", duration=2, voice="7", note_type="half")
    result = merged([("S", [chord]), ("A", [note("G", duration=2, note_type="half")])])
    assert [len(e.notes) for e in result.parts[0].measures[0].events] == [2, 1]
    incompatible = note("C", duration=2) + backup(2) + note("E", voice="2") + note("F", voice="2")
    with pytest.raises(ScoreError, match=r"measure 1, position 0.*: incompatible overlapping"):
        merged([("S", [incompatible]), ("A", [note(duration=2)])])


def test_rhythmic_and_spelled_pitch_differences_do_not_collapse():
    result = merged([("S", [note("C", duration=2, note_type="half")]),
                     ("A", [note("C") + note("D")])])
    assert {e.voice for e in result.parts[0].measures[0].events} == {"1", "2"}
    sharp_c = note("C").replace("<octave>", "<alter>1</alter><octave>")
    flat_d = note("D").replace("<octave>", "<alter>-1</alter><octave>")
    result = merged([("S", [sharp_c]), ("A", [flat_d])])
    assert len(result.parts[0].measures[0].events) == 2


def test_differing_grace_notes_cut_a_shared_sustained_note_with_ties():
    held = note("C", duration=4, note_type="whole")
    at_two = backup(2) + note("D", grace=True) + '<forward><duration>2</duration></forward>'
    result = merged([("S", [held + at_two]), ("A", [held])])
    events = result.parts[0].measures[0].events
    ordinary = [e for e in events if not e.grace]
    assert [(e.start, e.duration, e.voice) for e in ordinary] == [(0, 2, "1"), (2, 2, "1"), (2, 2, "2")]
    assert [[t.get("type") for t in e.notes[0].findall("tie")] for e in ordinary] == [["start"], ["stop"], ["stop"]]


def test_distinct_lyrics_numbered_stably_identical_lyrics_deduplicated():
    lyric = lambda text: f'<lyric number="1"><text>{text}</text></lyric>'
    upper = note(extra=lyric("hello")) + note("D", extra=lyric("same"))
    lower = note(extra=lyric("world")) + note("D", extra=lyric("same"))
    result = merged([("S", [upper]), ("A", [lower])])
    first, second = result.parts[0].measures[0].events
    assert [(l.get("number"), l.findtext("text")) for l in first.notes[0].findall("lyric")] == [("1", "hello"), ("2", "world")]
    assert len(second.notes[0].findall("lyric")) == 1


def test_annotations_directions_clef_metadata_and_stemless_values():
    marks = '<notations><articulations><staccato/></articulations></notations>'
    other = '<notations><articulations><accent/></articulations></notations>'
    direction = '<direction><direction-type><words>quiet</words></direction-type></direction>'
    whole = note("C", 3, 4, extra=marks, note_type="whole")
    result = merged([("S", [direction + whole]),
                     ("A", [direction + note("C", 3, 4, extra=other, note_type="whole")])])
    measure = result.parts[0].source.find("measure")
    assert measure.findtext(".//clef/sign") == "F"
    assert len(measure.findall("direction")) == 1
    assert measure.findtext("direction/voice") == "1"
    assert measure.find("note/stem") is None
    assert len(measure.findall("note/notations/articulations/*")) == 2
    assert result.root.findtext("work/work-title") == "Choir fixture"


@pytest.mark.parametrize("change,label", [
    ('<time><beats>3</beats><beat-type>4</beat-type></time>', "time"),
    ('<key><fifths>2</fifths></key>', "key"),
    ('<transpose><diatonic>1</diatonic><chromatic>2</chromatic></transpose>', "transpose"),
])
def test_compatibility_conflicts_identify_position(change, label):
    altered = note() + '<attributes>' + change + '</attributes>' + note()
    with pytest.raises(ScoreError, match=f"position 1: incompatible {label}"):
        merged([("S", [note() + note()]), ("A", [altered])])


def test_changing_divisions_exact_timing_and_tie_difference():
    upper = note(duration=1) + '<attributes><divisions>3</divisions></attributes>' + note("D", duration=1, extra='<time-modification><actual-notes>3</actual-notes><normal-notes>1</normal-notes></time-modification>')
    lower = note(duration=3) + note("D", duration=1, extra='<time-modification><actual-notes>3</actual-notes><normal-notes>1</normal-notes></time-modification>')
    original = score([("S", [upper]), ("A", [lower])])
    # Give the second part divisions=3 from its first event.
    attrs = original.parts[1].source.find("measure/attributes/divisions")
    attrs.text = "3"
    original = parse_score(original.root)
    root = merge_score(original, ["1+2"])
    validate_score(root)
    events = parse_score(root).parts[0].measures[0].events
    assert [(e.start, e.duration) for e in events] == [(0, 1), (1, Fraction(1, 3))]
    tied = note().replace('<voice>', '<tie type="start"/><voice>')
    result = merged([("S", [tied]), ("A", [note()])])
    assert len(result.parts[0].measures[0].events) == 2


def test_measure_boundary_conflict():
    with pytest.raises(ScoreError, match="incompatible measure boundaries"):
        merged([("S", [note(duration=2)]), ("A", [note()])])


def test_grace_sequence_retained_and_sliced_ids_are_unique():
    grace = note("D", grace=True) + note("E", grace=True)
    result = merged([("S", [grace + note()]), ("A", [grace + note()])])
    events = result.parts[0].measures[0].events
    assert [(e.notes[0].findtext("pitch/step"), e.grace) for e in events] == [("D", True), ("E", True), ("C", False)]
    held = note(duration=4, note_type="whole").replace('<note>', '<note id="held">')
    lower = note(duration=4, note_type="whole")
    at_two = backup(2) + note("D", grace=True) + '<forward><duration>2</duration></forward>'
    result = merged([("S", [held + at_two]), ("A", [lower])])
    ids = [n.get("id") for n in result.parts[0].source.iter() if n.get("id")]
    assert len(ids) == len(set(ids))


def test_slur_numbers_remain_distinct_after_common_start():
    start = '<notations><slur type="start" number="1"/></notations>'
    stop = '<notations><slur type="stop" number="1"/></notations>'
    upper = note(extra=start) + note("D", extra=stop)
    lower = note(extra=start) + note("E", extra=stop)
    result = merged([("S", [upper]), ("A", [lower])])
    nodes = result.parts[0].source.findall(".//slur")
    assert [(n.get("type"), n.get("number")) for n in nodes] == [
        ("start", "1"), ("start", "2"), ("stop", "1"), ("stop", "2")]


def test_display_names_instrument_players_and_lyric_order():
    upper = note(extra='<play><mute>off</mute></play>')
    lower = note(extra='<lyric number="1"><text>word</text></lyric>')
    original = score([("S", [upper]), ("A", [lower])])
    for index, part in enumerate(original.parts, 1):
        definition = part.definition
        display = ET.SubElement(definition, "part-name-display")
        ET.SubElement(display, "display-text").text = "OLD NAME"
        instrument = ET.SubElement(definition, "score-instrument", id=f"I{index}")
        ET.SubElement(instrument, "instrument-name").text = part.name
        player = ET.SubElement(definition, "player", id=f"Player{index}")
        ET.SubElement(player, "player-name").text = part.name
        midi = ET.SubElement(definition, "midi-instrument", id=f"I{index}")
        ET.SubElement(midi, "midi-channel").text = str(index)
        note_node = part.measures[0].events[0].notes[0]
        note_node.insert(2, ET.Element("instrument", id=f"I{index}"))
        note_node.set("id", f"N{index}")
    root = merge_score(original, ["1+2"])
    validate_score(root)
    definition = root.find("part-list/score-part")
    assert definition.find("part-name-display") is None
    assert [p.get("id") for p in definition.findall("player")] == ["Player1", "Player2"]
    assert [p.get("id") for p in definition.findall("midi-instrument")] == ["I1", "I2"]
    assert root.findtext("part/measure/note/lyric/text") == "word"
    assert root.find("part/measure/note/play") is not None


def test_hidden_signatures_and_explicit_zero_transpose_are_compatible():
    original = score([("S", [note()]), ("A", [note("D")])])
    attributes = original.parts[1].source.find("measure/attributes")
    for tag in ("time", "key"):
        node = attributes.find(tag)
        node.set("print-object", "no")
        node.set("id", "hidden-" + tag)
        node.set("default-y", "20")
    transpose = ET.SubElement(attributes, "transpose")
    ET.SubElement(transpose, "diatonic").text = "0"
    ET.SubElement(transpose, "chromatic").text = "0"
    ET.SubElement(transpose, "octave-change").text = "0"
    validate_score(original.root)
    original = parse_score(original.root)
    root = merge_score(original, ["1+2"])
    validate_score(root)
    assert len(parse_score(root).parts[0].measures[0].events) == 2


def test_source_implicit_instruments_become_explicit_and_overrides_survive():
    original = score([("S", [note() + note("E")]), ("A", [note("D") + note("F")])])
    for index, part in enumerate(original.parts, 1):
        instrument = ET.SubElement(part.definition, "score-instrument", id=f"I{index}")
        ET.SubElement(instrument, "instrument-name").text = part.name
    override = ET.SubElement(original.parts[1].definition, "score-instrument", id="AltOverride")
    ET.SubElement(override, "instrument-name").text = "Alternate alto"
    original.parts[1].measures[0].events[1].notes[0].insert(2, ET.Element("instrument", id="AltOverride"))
    root = merge_score(original, ["1+2"])
    validate_score(root)
    events = parse_score(root).parts[0].measures[0].events
    assert [(e.voice, e.notes[0].find("instrument").get("id")) for e in events] == [
        ("1", "I1"), ("1", "I1"), ("2", "I2"), ("2", "AltOverride")]


def test_visual_tie_difference_does_not_collapse():
    tied = note(extra='<notations><tied type="start"/></notations>')
    result = merged([("S", [tied]), ("A", [note()])])
    assert {e.voice for e in result.parts[0].measures[0].events} == {"1", "2"}
