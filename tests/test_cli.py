import os
import subprocess
import sys
from pathlib import Path

from mxml.cli import main
from mxml.io import read_score
from tests.helpers import note, score_xml


def test_split_merge_end_to_end(tmp_path):
    source = tmp_path / "input.musicxml"
    source.write_bytes(score_xml([
        ("Soprano", [note("E", duration=4, note_type="whole") + note("C", duration=4, chord=True, note_type="whole")]),
        ("Alto", [note("A", duration=4, note_type="whole")]),
    ]))
    split = tmp_path / "split.mxl"
    assert main(["split", str(source), "-o", str(split)]) == 0
    assert [part.name for part in read_score(split).parts] == ["Soprano", "Soprano 1", "Soprano 2", "Alto"]
    merged = tmp_path / "merged.musicxml"
    assert main(["merge", str(split), "-o", str(merged), "--pair", "2+3+4"]) == 0
    result = read_score(merged)
    assert [p.name for p in result.parts] == ["Soprano", "Soprano 1 / Soprano 2 / Alto"]
    assert {e.voice for e in result.parts[1].measures[0].events} == {"1", "2", "3"}


def test_cli_errors_are_stderr_and_leave_no_output(tmp_path, capsys):
    source = tmp_path / "input.xml"
    source.write_text("not xml")
    output = tmp_path / "output.xml"
    assert main(["split", str(source), "-o", str(output)]) == 2
    captured = capsys.readouterr()
    assert captured.out == ""
    assert "mxml:" in captured.err and "invalid XML" in captured.err
    assert not output.exists()


def test_module_entrypoint_and_help():
    environment = dict(os.environ, PYTHONPATH=str(Path(__file__).resolve().parents[1] / "tests" / "reference"))
    for option, expected in (("--version", "mxml 0.1.0"), ("--help", "split")):
        result = subprocess.run([sys.executable, "-m", "mxml", option], env=environment, text=True, capture_output=True)
        assert result.returncode == 0
        assert expected in result.stdout
