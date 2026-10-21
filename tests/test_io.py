import zipfile

import pytest
from lxml import etree as ET

from mxml.io import check_output, parse_xml, read_score, validate_score, write_score
from mxml.model import ScoreError
from tests.helpers import note, score, score_xml, sounding_events


@pytest.mark.parametrize("extension", [".mxml", ".musicxml", ".xml", ".mxl"])
def test_validated_roundtrip(tmp_path, extension):
    original = score_xml([("Soprano", [note(duration=4, note_type="whole")])])
    source = tmp_path / "input.musicxml"
    source.write_bytes(original)
    parsed = read_score(source)
    output = tmp_path / ("output" + extension)
    write_score(parsed.root, output, input_path=source)
    assert sounding_events(read_score(output).parts[0]) == sounding_events(parsed.parts[0])
    if extension == ".mxl":
        with zipfile.ZipFile(output) as archive:
            assert archive.namelist()[0] == "mimetype"
            assert archive.getinfo("mimetype").compress_type == zipfile.ZIP_STORED


def test_input_and_existing_output_are_protected(tmp_path):
    source = tmp_path / "input.xml"
    source.write_bytes(score_xml([("Soprano", [note()])]))
    with pytest.raises(ScoreError, match="Input replacement"):
        check_output(source, source, force=True)
    output = tmp_path / "output.xml"
    output.write_text("existing content")
    with pytest.raises(ScoreError, match="already exists"):
        write_score(read_score(source).root, output, input_path=source)
    assert output.read_text() == "existing content"
    write_score(read_score(source).root, output, input_path=source, force=True)
    assert read_score(output).parts[0].name == "Soprano"


def test_invalid_output_never_replaces_existing_file(tmp_path):
    source = tmp_path / "input.xml"
    source.write_bytes(score_xml([("Soprano", [note()])]))
    output = tmp_path / "output.xml"
    output.write_text("existing content")
    root = read_score(source).root
    root.find("part/measure/note/type").text = "invalid-type"
    with pytest.raises(ScoreError, match="schema validation"):
        write_score(root, output, input_path=source, force=True)
    assert output.read_text() == "existing content"
    assert not list(tmp_path.glob(".*.tmp"))


def test_entities_rejected_but_standard_external_doctype_is_allowed():
    with pytest.raises(ScoreError, match="entity declarations"):
        parse_xml(b'<!DOCTYPE score-partwise [<!ENTITY data SYSTEM "file:///etc/passwd">]><score-partwise>&data;</score-partwise>', "input")
    data = b'<!DOCTYPE score-partwise PUBLIC "-//Recordare//DTD MusicXML 4.0 Partwise//EN" "https://example.invalid/partwise.dtd">' + score_xml([("Soprano", [note()])])
    assert parse_xml(data, "input").tag == "score-partwise"


@pytest.mark.parametrize("name", ["../score.xml", "/score.xml", "C:/score.xml", "score\\file.xml"])
@pytest.mark.parametrize("separator", ["/", "\\"])
def test_unsafe_archive_paths_rejected(tmp_path, monkeypatch, name, separator):
    path = tmp_path / "bad.mxl"
    member = zipfile.ZipInfo()
    # Bypass ZipInfo's Windows separator normalization when creating malformed input.
    member.filename = name
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr(member, "bad")
    with monkeypatch.context() as platform:
        platform.setattr(zipfile.os, "sep", separator)
        with pytest.raises(ScoreError, match="Unsafe archive path"):
            read_score(path)


def test_missing_container_and_duplicate_archive_members_rejected(tmp_path):
    path = tmp_path / "bad.mxl"
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr("score.xml", "bad")
    with pytest.raises(ScoreError, match="container.xml.*missing"):
        read_score(path)
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr("score.xml", "one")
        with pytest.warns(UserWarning):
            archive.writestr("score.xml", "two")
    with pytest.raises(ScoreError, match="Duplicate"):
        read_score(path)


def test_duplicate_ids_rejected():
    parsed = score([("Soprano", [note()])])
    parsed.root.find("part/measure/note").set("id", "P1")
    with pytest.raises(ScoreError, match="duplicate ID"):
        validate_score(parsed.root)


def test_dangling_instrument_references_are_rejected():
    from mxml.model import insert_note_child

    parsed = score([("Soprano", [note()])])
    insert_note_child(parsed.root.find("part/measure/note"), ET.Element("instrument", id="missing"))
    with pytest.raises(ScoreError, match="unresolved reference 'missing'"):
        validate_score(parsed.root)
