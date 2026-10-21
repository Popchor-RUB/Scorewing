"""Run existing transformation assertions against the compiled Rust engine.

Invoke with PYTHONPATH=tests/reference MXML_RUST_ENGINE_BIN=... python3 -m pytest -p tests.rust_parity.
The Python implementation remains a test oracle during migration only.
"""
import os
import subprocess
import tempfile
from pathlib import Path

from lxml import etree as ET
from mxml.model import ScoreError


def transform(score, operation, arguments):
    binary = os.environ["MXML_RUST_ENGINE_BIN"]
    with tempfile.TemporaryDirectory() as directory:
        source = Path(directory) / "source.musicxml"
        source.write_bytes(ET.tostring(score.root))
        result = subprocess.run([binary, operation, str(source), *arguments],
                                capture_output=True, text=True)
        if result.returncode:
            raise ScoreError(result.stderr.removeprefix("mxml: ").strip())
        return ET.fromstring(result.stdout.encode(), ET.XMLParser(remove_blank_text=True))


def pytest_configure(config):
    import mxml.split
    import mxml.merge

    def split(score, mode="chords"):
        return transform(score, "split", [mode])

    def merge(score, pairs):
        # CLI selector compatibility is tested separately; compare engine behavior here.
        from mxml.merge import _select_groups
        groups = _select_groups(score, pairs)
        return transform(score, "merge", ["+".join(score.parts[index].id for index in group)
                                           for group in groups])

    mxml.split.split_score = split
    mxml.merge.merge_score = merge
