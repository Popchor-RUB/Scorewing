"""Exercise a packaged executable, including its offline schemas and MXL I/O."""

from pathlib import Path
import subprocess
import sys
from tempfile import TemporaryDirectory
import xml.etree.ElementTree as ET
from zipfile import ZipFile


SCORE = """<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="4.0">
  <part-list>
    <score-part id="P1"><part-name>Soprano</part-name></score-part>
    <score-part id="P2"><part-name>Alto</part-name></score-part>
  </part-list>
  <part id="P1"><measure number="1">
    <attributes><divisions>1</divisions><key><fifths>0</fifths></key>
      <time><beats>2</beats><beat-type>4</beat-type></time>
      <clef><sign>G</sign><line>2</line></clef></attributes>
    <note><pitch><step>C</step><octave>4</octave></pitch>
      <duration>1</duration><type>quarter</type></note>
    <note><pitch><step>E</step><octave>4</octave></pitch>
      <duration>1</duration><type>quarter</type></note>
    <note><chord/><pitch><step>G</step><octave>4</octave></pitch>
      <duration>1</duration><type>quarter</type></note>
  </measure></part>
  <part id="P2"><measure number="1">
    <attributes><divisions>1</divisions><key><fifths>0</fifths></key>
      <time><beats>2</beats><beat-type>4</beat-type></time>
      <clef><sign>G</sign><line>2</line></clef></attributes>
    <note><pitch><step>C</step><octave>4</octave></pitch>
      <duration>1</duration><type>quarter</type></note>
    <note><pitch><step>D</step><octave>4</octave></pitch>
      <duration>1</duration><type>quarter</type></note>
  </measure></part>
</score-partwise>
"""


def main():
    executable = Path(sys.argv[1]).resolve()
    subprocess.run([str(executable), "--help"], check=True, capture_output=True)
    with TemporaryDirectory() as directory:
        root = Path(directory)
        source = root / "choir.musicxml"
        source.write_text(SCORE, encoding="utf-8")
        split = root / "split.musicxml"
        merged = root / "merged.mxl"
        for arguments in [
            ["split", str(source), "-o", str(split)],
            ["merge", str(source), "-o", str(merged), "--pair", "1+2"],
            ["split", str(merged), "-o", str(root / "roundtrip.musicxml")],
        ]:
            result = subprocess.run([str(executable), *arguments], capture_output=True, text=True)
            assert result.returncode == 0, result.stderr
        tree = ET.parse(split)
        assert len(tree.findall("part")) == 4
        assert [p.findtext("part-name") for p in tree.findall("part-list/score-part")] == [
            "Soprano", "Soprano 1", "Soprano 2", "Alto"
        ]
        with ZipFile(merged) as archive:
            container = ET.fromstring(archive.read("META-INF/container.xml"))
            score_path = container.find("{*}rootfiles/{*}rootfile").attrib["full-path"]
            result = ET.fromstring(archive.read(score_path))
        assert len(result.findall("part")) == 1
        notes = result.findall("part/measure/note")
        assert {n.findtext("voice") for n in notes} == {"1", "2"}
        assert all(n.findtext("stem") == ("up" if n.findtext("voice") == "1" else "down") for n in notes)
        assert len(ET.parse(root / "roundtrip.musicxml").findall("part")) >= 1
    print("Standalone split, merge, offline validation, and MXL roundtrip passed.")


if __name__ == "__main__":
    main()
