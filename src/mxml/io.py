"""Secure MusicXML loading, offline validation, and atomic output."""

from __future__ import annotations

import io
import os
import stat
import tempfile
import zipfile
from functools import lru_cache
from pathlib import Path, PurePosixPath

from lxml import etree as ET

from .model import Score, ScoreError, parse_score

MAX_XML_BYTES = 64 * 1024 * 1024
EXTENSIONS = {".mxml", ".musicxml", ".xml", ".mxl"}
ID_REFERENCE_TAGS = {"part", "midi-device", "midi-instrument", "play", "instrument-change", "instrument", "instrument-link"}


def parse_xml(data: bytes, context: str) -> ET._Element:
    parser = ET.XMLParser(resolve_entities=False, load_dtd=False, no_network=True,
                          remove_blank_text=True, strip_cdata=False)
    try:
        root = ET.fromstring(data, parser)
    except ET.XMLSyntaxError as error:
        raise ScoreError(f"{context}: invalid XML: {error}") from None
    dtd = root.getroottree().docinfo.internalDTD
    if dtd is not None and any(dtd.iterentities()):
        raise ScoreError(f"{context}: XML entity declarations are not allowed")
    return root


def _archive_path(name: str) -> None:
    path = PurePosixPath(name)
    if (not name or "\\" in name or path.is_absolute() or ".." in path.parts
            or ":" in name or "\x00" in name or name.startswith("./")):
        raise ScoreError(f"Unsafe archive path: {name!r}")


def _read_member(archive: zipfile.ZipFile, name: str) -> bytes:
    _archive_path(name)
    try:
        info = archive.getinfo(name)
        if info.file_size > MAX_XML_BYTES:
            raise ScoreError(f"Archive member {name!r} exceeds the 64 MiB XML limit")
        with archive.open(info) as stream:
            data = stream.read(MAX_XML_BYTES + 1)
        if len(data) > MAX_XML_BYTES:
            raise ScoreError(f"Archive member {name!r} exceeds the 64 MiB XML limit")
        return data
    except KeyError:
        raise ScoreError(f"Archive member {name!r} is missing") from None


def read_score(path: str | Path) -> Score:
    path = Path(path)
    with path.open("rb") as stream:
        data = stream.read(MAX_XML_BYTES + 1)
    if len(data) > MAX_XML_BYTES:
        raise ScoreError("Input exceeds the 64 MiB file limit")
    if zipfile.is_zipfile(io.BytesIO(data)):
        try:
            with zipfile.ZipFile(io.BytesIO(data)) as archive:
                seen = set()
                for info in archive.infolist():
                    _archive_path(info.filename)
                    if info.filename in seen or stat.S_ISLNK(info.external_attr >> 16):
                        raise ScoreError(f"Duplicate or symbolic archive member: {info.filename!r}")
                    seen.add(info.filename)
                container = parse_xml(_read_member(archive, "META-INF/container.xml"), "MXL container")
                roots = container.findall("{*}rootfiles/{*}rootfile")
                selected = next((r for r in roots if r.get("media-type") in
                                 (None, "application/vnd.recordare.musicxml+xml")), None)
                if selected is None or not selected.get("full-path"):
                    raise ScoreError("MXL container has no MusicXML rootfile")
                data = _read_member(archive, selected.get("full-path"))
        except (zipfile.BadZipFile, RuntimeError, NotImplementedError) as error:
            raise ScoreError(f"Invalid or unsupported MXL archive: {error}") from None
    elif path.suffix.lower() == ".mxl":
        raise ScoreError("An .mxl input must be a compressed MusicXML archive")
    return parse_score(parse_xml(data, str(path)))


@lru_cache(maxsize=1)
def _schema() -> ET.XMLSchema:
    schema_path = Path(__file__).with_name("schema") / "musicxml.xsd"
    parser = ET.XMLParser(resolve_entities=False, no_network=True, load_dtd=False)
    return ET.XMLSchema(ET.parse(str(schema_path), parser))


def validate_score(root: ET._Element) -> None:
    parse_score(root)
    identifiers = set()
    references = []
    for element in root.iter():
        identifier = element.get("id")
        if identifier is not None:
            if element.tag in ID_REFERENCE_TAGS:
                references.append((element, identifier))
            elif identifier in identifiers:
                raise ScoreError(f"Generated score contains duplicate ID {identifier!r}")
            else:
                identifiers.add(identifier)
        if element.get("player") is not None:
            references.append((element, element.get("player")))
        for attribute, value in element.attrib.items():
            if attribute.rsplit("}", 1)[-1] == "href" and value.startswith("#"):
                references.append((element, value[1:]))
    for element, identifier in references:
        if identifier not in identifiers:
            raise ScoreError(f"Generated score contains unresolved reference {identifier!r} at {root.getroottree().getpath(element)}")
    schema = _schema()
    if not schema.validate(root):
        error = schema.error_log.last_error
        raise ScoreError(f"Generated MusicXML failed schema validation: {error}")


def check_output(input_path: str | Path, output_path: str | Path, force: bool = False) -> None:
    source, output = Path(input_path), Path(output_path)
    if output.suffix.lower() not in EXTENSIONS:
        raise ScoreError("Output extension must be .mxml, .musicxml, .xml, or .mxl")
    if source.resolve() == output.resolve() or output.exists() and source.samefile(output):
        raise ScoreError("Input replacement is refused; choose a different output path")
    if os.path.lexists(output) and not force:
        raise ScoreError(f"Output {str(output)!r} already exists; use --force to replace it")


def write_score(root: ET._Element, output_path: str | Path, *, input_path: str | Path,
                force: bool = False) -> None:
    output = Path(output_path)
    check_output(input_path, output, force)
    validate_score(root)
    data = ET.tostring(root, encoding="UTF-8", xml_declaration=True, pretty_print=True)
    temporary = None
    try:
        descriptor, temporary = tempfile.mkstemp(prefix=f".{output.name}.", suffix=".tmp", dir=output.parent)
        with os.fdopen(descriptor, "wb") as stream:
            if output.suffix.lower() == ".mxl":
                with zipfile.ZipFile(stream, "w", compression=zipfile.ZIP_DEFLATED) as archive:
                    archive.writestr("mimetype", "application/vnd.recordare.musicxml", compress_type=zipfile.ZIP_STORED)
                    archive.writestr("META-INF/container.xml", (
                        '<?xml version="1.0" encoding="UTF-8"?>'
                        '<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">'
                        '<rootfiles><rootfile full-path="score.musicxml" '
                        'media-type="application/vnd.recordare.musicxml+xml"/></rootfiles></container>'
                    ))
                    archive.writestr("score.musicxml", data)
            else:
                stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        # Recheck the source alias immediately before installing the complete output.
        check_output(input_path, output, force)
        if force:
            os.replace(temporary, output)
        else:
            # Linking in the same directory is atomic and refuses a concurrently created output.
            os.link(temporary, output)
    finally:
        if temporary is not None:
            try:
                os.unlink(temporary)
            except FileNotFoundError:
                pass
