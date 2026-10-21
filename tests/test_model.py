from fractions import Fraction

import pytest
from lxml import etree as ET

from mxml.model import ScoreError, parse_score, render_measure, slice_event
from tests.helpers import backup, note, score, sounding_events


def test_backup_forward_chords_and_changed_divisions_roundtrip():
    body = (note("C", duration=2) + note("E", duration=2, chord=True) + backup(2)
            + '<attributes><divisions>3</divisions></attributes>'
            + note("G", duration=3, voice="2")
            + '<forward><duration>3</duration></forward>')
    parsed = score([("Soprano", [body])], divisions=2)
    part = parsed.parts[0]
    assert [(e.start, e.duration) for e in part.measures[0].events] == [(0, 1), (0, 1)]
    before = sounding_events(part)
    part.source.replace(part.measures[0].source, render_measure(part.measures[0], part.measures[0].events))
    after = parse_score(parsed.root)
    assert sounding_events(after.parts[0]) == before
    assert after.parts[0].measures[0].length == 2


def test_slice_preserves_sounding_time_and_adds_ties():
    parsed = score([("Soprano", [note(duration=4, note_type="whole", extra='<lyric><text>Ah</text></lyric>')])])
    event = parsed.parts[0].measures[0].events[0]
    left = slice_event(event, Fraction(0), Fraction(1))
    right = slice_event(event, Fraction(1), Fraction(4))
    assert left.notes[0].findtext("type") == "quarter"
    assert right.notes[0].findtext("type") == "half"
    assert right.notes[0].find("dot") is not None
    assert [t.get("type") for t in left.notes[0].findall("tie")] == ["start"]
    assert [t.get("type") for t in right.notes[0].findall("tie")] == ["stop"]
    assert right.notes[0].find("lyric") is None


@pytest.mark.parametrize("body,match", [
    (backup(1), "backup crosses"),
    (note(chord=True), "chord tones"),
    ('<attributes><staves>2</staves></attributes>' + note(), "multi-staff"),
    ('<attributes><clef number="2"><sign>F</sign><line>4</line></clef></attributes>' + note(), "multi-staff"),
    ('<attributes><divisions>0</divisions></attributes>' + note(), "positive"),
])
def test_invalid_input_has_context(body, match):
    with pytest.raises(ScoreError, match=match):
        score([("Soprano", [body])])


def test_timewise_rejected():
    with pytest.raises(ScoreError, match="score-timewise"):
        parse_score(ET.fromstring(b"<score-timewise/>"))


def test_decimal_divisions_and_durations_are_exact():
    parsed = score([("Soprano", [note(duration="1.5")])], divisions="1.5")
    measure = parsed.parts[0].measures[0]
    assert measure.events[0].duration == 1
    parsed.parts[0].source.replace(measure.source, render_measure(measure, measure.events))
    assert parse_score(parsed.root).parts[0].measures[0].events[0].duration == 1


def test_slicing_retains_relationship_styles_and_slur_endpoint():
    body = note(duration=2, note_type="half", extra=(
        '<notations><tied type="stop" orientation="under"/>'
        '<slur type="stop" number="1"/></notations>'))
    body = body.replace("<voice>", '<tie type="stop"/><voice>')
    event = score([("Soprano", [body])]).parts[0].measures[0].events[0]
    unchanged = slice_event(event, Fraction(0), Fraction(2))
    assert ET.tostring(unchanged.notes[0]) == ET.tostring(event.notes[0])
    first = slice_event(event, Fraction(0), Fraction(1))
    last = slice_event(event, Fraction(1), Fraction(2))
    assert first.notes[0].find('notations/tied[@type="stop"]').get("orientation") == "under"
    assert first.notes[0].find("notations/slur") is not None
    assert last.notes[0].find("notations/slur") is None
