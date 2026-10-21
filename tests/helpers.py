from fractions import Fraction

from lxml import etree as ET

from mxml.model import parse_score


def note(step="C", octave=4, duration=1, voice="1", *, chord=False, rest=False,
         extra="", note_type="quarter", grace=False):
    sound = "<rest/>" if rest else f"<pitch><step>{step}</step><octave>{octave}</octave></pitch>"
    return ("<note>" + ("<grace/>" if grace else "") + ("<chord/>" if chord else "") + sound
            + ("" if grace else f"<duration>{duration}</duration>")
            + f"<voice>{voice}</voice><type>{note_type}</type>{extra}</note>")


def score_xml(parts, *, divisions=1, beats=4, beat_type=4):
    """parts is a list of (name, list of measure XML bodies)."""
    definitions = "".join(f'<score-part id="P{i}"><part-name>{name}</part-name></score-part>'
                          for i, (name, _) in enumerate(parts, 1))
    contents = []
    for i, (_, measures) in enumerate(parts, 1):
        items = []
        for number, body in enumerate(measures, 1):
            attributes = (f"<attributes><divisions>{divisions}</divisions><key><fifths>0</fifths></key>"
                          f"<time><beats>{beats}</beats><beat-type>{beat_type}</beat-type></time>"
                          "<clef><sign>G</sign><line>2</line></clef></attributes>") if number == 1 else ""
            items.append(f'<measure number="{number}">{attributes}{body}</measure>')
        contents.append(f'<part id="P{i}">{"".join(items)}</part>')
    return (f'<score-partwise version="4.0"><work><work-title>Choir fixture</work-title></work>'
            f'<part-list>{definitions}</part-list>{"".join(contents)}</score-partwise>').encode()


def score(parts, **kwargs):
    return parse_score(ET.fromstring(score_xml(parts, **kwargs)))


def sounding_events(part):
    return [(i, e.start, e.duration, e.voice,
             tuple((n.findtext("pitch/step"), n.findtext("pitch/alter", "0"), n.findtext("pitch/octave"))
                   if n.find("rest") is None else ("rest",) for n in e.notes))
            for i, measure in enumerate(part.measures) for e in measure.events if not e.grace]


def backup(duration):
    return f"<backup><duration>{duration}</duration></backup>"
