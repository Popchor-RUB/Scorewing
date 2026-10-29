use crate::xml::*;
use crate::{parse_score, Result, Score, ScoreError};
use std::collections::HashSet;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use xmltree::{Element, EmitterConfig, XMLNode};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

pub const MAX_XML_BYTES: usize = 64 * 1024 * 1024;
pub const EXTENSIONS: &[&str] = &["mxml", "musicxml", "xml", "mxl"];
const ID_REFERENCE_TAGS: &[&str] = &[
    "part",
    "midi-device",
    "midi-instrument",
    "play",
    "instrument-change",
    "instrument",
    "instrument-link",
];

pub fn parse_xml(data: &[u8], context: &str) -> Result<Element> {
    if data.len() > MAX_XML_BYTES {
        return Err(ScoreError::new("Input exceeds the 64 MiB XML limit"));
    }
    // Also detect declarations in UTF-16/32 before any XML parser can expand them.
    let ascii: std::borrow::Cow<'_, [u8]> = if data.contains(&0) {
        std::borrow::Cow::Owned(data.iter().copied().filter(|c| *c != 0).collect())
    } else {
        std::borrow::Cow::Borrowed(data)
    };
    let mut offset = 0;
    while offset < ascii.len() {
        let suffix = &ascii[offset..];
        let skip = if suffix.starts_with(b"<!--") {
            Some((4, b"-->".as_slice()))
        } else if suffix.starts_with(b"<![CDATA[") {
            Some((9, b"]]>".as_slice()))
        } else {
            None
        };
        if let Some((start, end)) = skip {
            offset += suffix[start..]
                .windows(end.len())
                .position(|w| w == end)
                .map(|n| start + n + end.len())
                .unwrap_or(suffix.len());
            continue;
        }
        if suffix.starts_with(b"<!ENTITY") {
            return Err(ScoreError(format!(
                "{context}: XML entity declarations are not allowed"
            )));
        }
        offset += 1;
    }
    let mut stack: Vec<Element> = Vec::new();
    let mut root = None;
    let reader = ::xml::reader::ParserConfig::new()
        .ignore_comments(false)
        .cdata_to_characters(false)
        .create_reader(data);
    for event in reader {
        match event.map_err(|e| ScoreError(format!("{context}: invalid XML: {e}")))? {
            ::xml::reader::XmlEvent::StartElement {
                name,
                attributes,
                namespace,
            } => {
                if stack.len() >= 256 {
                    return Err(ScoreError(format!(
                        "{context}: XML nesting exceeds 256 levels"
                    )));
                }
                let mut element = Element::new(&name.local_name);
                element.prefix = name.prefix;
                element.namespace = name.namespace;
                element.namespaces = Some(namespace);
                for attr in attributes {
                    let key = attr
                        .name
                        .prefix
                        .map(|prefix| format!("{prefix}:{}", attr.name.local_name))
                        .unwrap_or(attr.name.local_name);
                    element.attributes.insert(key, attr.value);
                }
                stack.push(element);
            }
            ::xml::reader::XmlEvent::EndElement { .. } => {
                let element = stack
                    .pop()
                    .ok_or_else(|| ScoreError(format!("{context}: invalid XML")))?;
                if let Some(parent) = stack.last_mut() {
                    append(parent, element);
                } else {
                    root = Some(element);
                }
            }
            ::xml::reader::XmlEvent::Characters(value)
            | ::xml::reader::XmlEvent::Whitespace(value) => {
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(XMLNode::Text(value));
                }
            }
            ::xml::reader::XmlEvent::CData(value) => {
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(XMLNode::CData(value));
                }
            }
            ::xml::reader::XmlEvent::Comment(value) => {
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(XMLNode::Comment(value));
                }
            }
            ::xml::reader::XmlEvent::ProcessingInstruction { name, data } => {
                if let Some(parent) = stack.last_mut() {
                    parent
                        .children
                        .push(XMLNode::ProcessingInstruction(name, data));
                }
            }
            _ => {}
        }
    }
    root.ok_or_else(|| ScoreError(format!("{context}: invalid XML: no document element")))
}
fn archive_path(name: &str) -> Result<()> {
    if name.is_empty()
        || name.contains(['\\', ':', '\0'])
        || name.starts_with('/')
        || name.starts_with("./")
        || name.split('/').any(|p| p == "..")
    {
        return Err(ScoreError(format!("Unsafe archive path: {name:?}")));
    }
    Ok(())
}
fn read_member(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Vec<u8>> {
    archive_path(name)?;
    let member = archive.by_name(name).map_err(|e| match e {
        zip::result::ZipError::FileNotFound => {
            ScoreError(format!("Archive member {name:?} is missing"))
        }
        _ => e.into(),
    })?;
    if member.size() > MAX_XML_BYTES as u64 {
        return Err(ScoreError(format!(
            "Archive member {name:?} exceeds the 64 MiB XML limit"
        )));
    }
    let mut data = Vec::new();
    member
        .take(MAX_XML_BYTES as u64 + 1)
        .read_to_end(&mut data)?;
    if data.len() > MAX_XML_BYTES {
        return Err(ScoreError(format!(
            "Archive member {name:?} exceeds the 64 MiB XML limit"
        )));
    }
    Ok(data)
}
fn validate_directory(bytes: &[u8], start: u64, unique_count: usize) -> Result<()> {
    // zip deduplicates names internally; inspect central entries before that can hide unsafe members.
    let mut cursor =
        usize::try_from(start).map_err(|_| ScoreError::new("Invalid MXL directory offset"))?;
    let mut seen = HashSet::new();
    let mut count = 0;
    while bytes.get(cursor..cursor + 4) == Some(b"PK\x01\x02") {
        let fixed = bytes
            .get(cursor..cursor + 46)
            .ok_or_else(|| ScoreError::new("Invalid MXL central directory"))?;
        let short = |at| u16::from_le_bytes([fixed[at], fixed[at + 1]]) as usize;
        let name_length = short(28);
        let extra_length = short(30);
        let comment_length = short(32);
        let end = cursor + 46 + name_length + extra_length + comment_length;
        let name = bytes
            .get(cursor + 46..cursor + 46 + name_length)
            .ok_or_else(|| ScoreError::new("Invalid MXL member name"))?;
        archive_path(&String::from_utf8_lossy(name))?;
        let attributes = u32::from_le_bytes(fixed[38..42].try_into().unwrap());
        if !seen.insert(name.to_vec()) || ((attributes >> 16) & 0o170000) == 0o120000 {
            return Err(ScoreError(format!(
                "Duplicate or symbolic archive member: {:?}",
                String::from_utf8_lossy(name)
            )));
        }
        if end > bytes.len() {
            return Err(ScoreError::new("Invalid MXL central directory"));
        }
        count += 1;
        cursor = end;
    }
    if count != unique_count {
        return Err(ScoreError::new("Duplicate archive members are not allowed"));
    }
    Ok(())
}
pub fn read_score(path: &Path) -> Result<Score> {
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_XML_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    read_score_bytes(&bytes, &path.to_string_lossy())
}
pub fn read_score_bytes(bytes: &[u8], context: &str) -> Result<Score> {
    if bytes.len() > MAX_XML_BYTES {
        return Err(ScoreError::new("Input exceeds the 64 MiB file limit"));
    }
    let archive = ZipArchive::new(Cursor::new(bytes));
    let data = if let Ok(mut archive) = archive {
        validate_directory(bytes, archive.central_directory_start(), archive.len())?;
        let mut seen = HashSet::new();
        for index in 0..archive.len() {
            let member = archive.by_index(index)?;
            archive_path(member.name())?;
            if !seen.insert(member.name().to_owned())
                || member
                    .unix_mode()
                    .is_some_and(|mode| mode & 0o170000 == 0o120000)
            {
                return Err(ScoreError(format!(
                    "Duplicate or symbolic archive member: {:?}",
                    member.name()
                )));
            }
        }
        let container = parse_xml(
            &read_member(&mut archive, "META-INF/container.xml")?,
            "MXL container",
        )?;
        let selected = child(&container, "rootfiles")
            .and_then(|r| {
                children(r, "rootfile").find(|r| {
                    r.attributes
                        .get("media-type")
                        .is_none_or(|t| t == "application/vnd.recordare.musicxml+xml")
                })
            })
            .and_then(|r| r.attributes.get("full-path"))
            .filter(|p| !p.is_empty())
            .ok_or_else(|| ScoreError::new("MXL container has no MusicXML rootfile"))?;
        read_member(&mut archive, selected)?
    } else {
        if Path::new(context)
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("mxl"))
        {
            return Err(ScoreError::new(
                "An .mxl input must be a compressed MusicXML archive",
            ));
        }
        bytes.to_vec()
    };
    parse_score(parse_xml(&data, context)?)
}
pub fn serialize_xml(root: &Element) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    root.write_with_config(
        &mut bytes,
        EmitterConfig::new()
            .perform_indent(true)
            .write_document_declaration(true),
    )
    .map_err(|e| ScoreError(e.to_string()))?;
    Ok(bytes)
}
pub fn validate_score(root: &Element) -> Result<()> {
    parse_score(root.clone())?;
    let mut identifiers = HashSet::new();
    let mut references = Vec::new();
    let mut duplicate = None;
    visit(root, &mut |node| {
        if let Some(id) = node.attributes.get("id") {
            if ID_REFERENCE_TAGS.contains(&node.name.as_str()) {
                references.push((node.name.clone(), id.clone()));
            } else if !identifiers.insert(id.clone()) {
                duplicate = Some(id.clone());
            }
        }
        if let Some(player) = node.attributes.get("player") {
            references.push((node.name.clone(), player.clone()));
        }
        for (key, value) in &node.attributes {
            if key.rsplit(['}', ':']).next() == Some("href") && value.starts_with('#') {
                references.push((node.name.clone(), value[1..].into()));
            }
        }
    });
    if let Some(id) = duplicate {
        return Err(ScoreError(format!(
            "Generated score contains duplicate ID {id:?}"
        )));
    }
    for (name, id) in references {
        if !identifiers.contains(&id) {
            return Err(ScoreError(format!(
                "Generated score contains unresolved reference '{id}' at {name}"
            )));
        }
    }
    let directory = tempfile::tempdir()?;
    for (name, data) in [
        (
            "musicxml.xsd",
            include_bytes!("../schema/musicxml.xsd").as_slice(),
        ),
        ("xml.xsd", include_bytes!("../schema/xml.xsd").as_slice()),
        (
            "xlink.xsd",
            include_bytes!("../schema/xlink.xsd").as_slice(),
        ),
    ] {
        std::fs::write(directory.path().join(name), data)?;
    }
    let path = directory.path().join("musicxml.xsd");
    let mut parser = libxml::schemas::SchemaParserContext::from_file(
        path.to_str()
            .ok_or_else(|| ScoreError::new("Schema path is not UTF-8"))?,
    );
    let mut validator = libxml::schemas::SchemaValidationContext::from_parser(&mut parser)
        .map_err(|errors| {
            ScoreError(format!(
                "Bundled MusicXML schema could not be loaded: {errors:?}"
            ))
        })?;
    let options = libxml::parser::ParserOptions {
        no_net: true,
        recover: false,
        ..Default::default()
    };
    let document = libxml::parser::Parser::default()
        .parse_string_with_options(serialize_xml(root)?, options)
        .map_err(|e| ScoreError(format!("Generated MusicXML could not be parsed: {e}")))?;
    validator.validate_document(&document).map_err(|errors| {
        ScoreError(format!(
            "Generated MusicXML failed schema validation: {errors:?}"
        ))
    })
}
fn resolved(path: &Path) -> Result<PathBuf> {
    if let Ok(path) = path.canonicalize() {
        return Ok(path);
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let parent = absolute.parent().unwrap_or(Path::new("."));
    let parent = parent.canonicalize()?;
    Ok(parent.join(
        absolute
            .file_name()
            .ok_or_else(|| ScoreError::new("Output must name a file"))?,
    ))
}
pub fn check_output(input: &Path, output: &Path, force: bool) -> Result<()> {
    if !output
        .extension()
        .and_then(|x| x.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
    {
        return Err(ScoreError::new(
            "Output extension must be .mxml, .musicxml, .xml, or .mxl",
        ));
    }
    if resolved(input)? == resolved(output)?
        || same_file::is_same_file(input, output).unwrap_or(false)
    {
        return Err(ScoreError::new(
            "Input replacement is refused; choose a different output path",
        ));
    }
    if output.symlink_metadata().is_ok() && !force {
        return Err(ScoreError(format!(
            "Output {output:?} already exists; use --force to replace it"
        )));
    }
    Ok(())
}
pub fn write_score(root: &Element, output: &Path, input: &Path, force: bool) -> Result<()> {
    check_output(input, output, force)?;
    validate_score(root)?;
    let data = serialize_xml(root)?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::Builder::new()
        .prefix(".scorewing-")
        .suffix(".tmp")
        .tempfile_in(parent)?;
    if output
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("mxl"))
    {
        let mut archive = ZipWriter::new(temporary.as_file_mut());
        archive.start_file(
            "mimetype",
            SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
        )?;
        archive.write_all(b"application/vnd.recordare.musicxml")?;
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        archive.start_file("META-INF/container.xml", options)?;
        archive.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="score.musicxml" media-type="application/vnd.recordare.musicxml+xml"/></rootfiles></container>"#)?;
        archive.start_file("score.musicxml", options)?;
        archive.write_all(&data)?;
        archive.finish()?;
    } else {
        temporary.write_all(&data)?;
    }
    temporary.as_file_mut().flush()?;
    temporary.as_file().sync_all()?;
    check_output(input, output, force)?;
    if force {
        temporary
            .persist(output)
            .map_err(|e| ScoreError(e.error.to_string()))?;
    } else {
        temporary
            .persist_noclobber(output)
            .map_err(|e| ScoreError(e.error.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn score() -> Element {
        parse_xml(br#"<score-partwise version="4.0"><part-list><score-part id="P1"><part-name>Voice</part-name></score-part></part-list><part id="P1"><measure number="1"><attributes><divisions>1</divisions></attributes><note><pitch><step>C</step><octave>4</octave></pitch><duration>1</duration><type>quarter</type></note></measure></part></score-partwise>"#,"test").unwrap()
    }
    #[test]
    fn rejects_entities_but_accepts_external_doctype() {
        assert!(parse_xml(br#"<!DOCTYPE score-partwise [<!ENTITY data SYSTEM "file:///etc/passwd">]><score-partwise>&data;</score-partwise>"#,"test").unwrap_err().to_string().contains("entity declarations"));
        assert!(parse_xml(br#"<!DOCTYPE score-partwise SYSTEM "https://example.invalid/test.dtd"><score-partwise/>"#,"test").is_ok());
        assert_eq!(parse_xml(br#"<!DOCTYPE score-partwise PUBLIC "-//Recordare//DTD MusicXML 4.0 Partwise//EN" "https://example.invalid/partwise.dtd"><score-partwise/>"#, "test").unwrap().name, "score-partwise");
    }
    #[test]
    fn rejects_unsafe_paths() {
        for p in [
            "../score.xml",
            "/score.xml",
            "C:/score.xml",
            "score\\file.xml",
            "./score.xml",
        ] {
            assert!(archive_path(p).is_err());
        }
    }
    #[test]
    fn validated_plain_and_archive_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.xml");
        let mut root = score();
        let part = child_mut(&mut root, "part").unwrap();
        let measure = child_mut(part, "measure").unwrap();
        let note = child_mut(measure, "note").unwrap();
        set_child(note, "duration", "4");
        set_child(note, "type", "whole");
        std::fs::write(&input, serialize_xml(&root).unwrap()).unwrap();
        let original = read_score(&input).unwrap();
        let before = &original.parts[0].measures[0].events[0];
        for ext in EXTENSIONS {
            let output = dir.path().join(format!("output.{ext}"));
            write_score(&root, &output, &input, false).unwrap();
            let parsed = read_score(&output).unwrap();
            assert_eq!(parsed.parts[0].name(), "Voice");
            assert_eq!(parsed.parts[0].measures.len(), 1);
            assert_eq!(parsed.parts[0].measures[0].events.len(), 1);
            let after = &parsed.parts[0].measures[0].events[0];
            assert_eq!(after.start, before.start);
            assert_eq!(after.duration, before.duration);
            assert_eq!(after.voice, before.voice);
            assert_eq!(after.notes.len(), before.notes.len());
            assert_eq!(
                child(&after.notes[0], "pitch"),
                child(&before.notes[0], "pitch")
            );
            if *ext == "mxl" {
                let mut archive = ZipArchive::new(std::fs::File::open(&output).unwrap()).unwrap();
                let first = archive.by_index(0).unwrap();
                assert_eq!(first.name(), "mimetype");
                assert_eq!(first.compression(), CompressionMethod::Stored);
            }
        }
        assert!(check_output(&input, &input, true)
            .unwrap_err()
            .to_string()
            .contains("Input replacement"));
    }
    #[test]
    fn invalid_output_preserves_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.xml");
        let output = dir.path().join("output.xml");
        std::fs::write(&input, serialize_xml(&score()).unwrap()).unwrap();
        std::fs::write(&output, "keep").unwrap();
        let mut root = score();
        let part = child_mut(&mut root, "part").unwrap();
        let measure = child_mut(part, "measure").unwrap();
        set_child(child_mut(measure, "note").unwrap(), "type", "invalid-type");
        assert!(write_score(&root, &output, &input, true)
            .unwrap_err()
            .to_string()
            .contains("schema validation"));
        assert_eq!(std::fs::read_to_string(output).unwrap(), "keep");
        assert!(std::fs::read_dir(dir.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        }));
    }
    #[test]
    fn ids_and_references_are_checked() {
        let mut root = score();
        let part = child_mut(&mut root, "part").unwrap();
        let measure = child_mut(part, "measure").unwrap();
        child_mut(measure, "note")
            .unwrap()
            .attributes
            .insert("id".into(), "P1".into());
        assert!(validate_score(&root)
            .unwrap_err()
            .to_string()
            .contains("duplicate ID"));
    }
    #[test]
    fn dangling_instrument_references_are_rejected() {
        let mut root = score();
        let part = child_mut(&mut root, "part").unwrap();
        let measure = child_mut(part, "measure").unwrap();
        let mut instrument = Element::new("instrument");
        instrument.attributes.insert("id".into(), "missing".into());
        insert_note_child(child_mut(measure, "note").unwrap(), instrument);
        assert!(validate_score(&root)
            .unwrap_err()
            .to_string()
            .contains("unresolved reference 'missing'"));
    }
    #[test]
    fn rejects_hostile_archives_before_loading_score() {
        for name in [
            "../score.xml",
            "/score.xml",
            "C:/score.xml",
            "score\\file.xml",
        ] {
            let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
            archive
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            archive.write_all(b"bad").unwrap();
            let bytes = archive.finish().unwrap().into_inner();
            assert!(read_score_bytes(&bytes, "test.mxl")
                .unwrap_err()
                .to_string()
                .contains("Unsafe archive path"));
        }
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        archive
            .start_file("score.xml", SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"bad").unwrap();
        let bytes = archive.finish().unwrap().into_inner();
        assert!(read_score_bytes(&bytes, "test.mxl")
            .unwrap_err()
            .to_string()
            .contains("container.xml"));
        assert!(read_score_bytes(b"<score-partwise/>", "test.mxl")
            .unwrap_err()
            .to_string()
            .contains("compressed MusicXML"));
    }
    #[test]
    fn bounds_depth_and_entity_decoding() {
        let deep = format!("{}{}", "<x>".repeat(257), "</x>".repeat(257));
        assert!(parse_xml(deep.as_bytes(), "test")
            .unwrap_err()
            .to_string()
            .contains("nesting"));
        let malicious="<!DOCTYPE score-partwise [<!ENTITY data SYSTEM 'file:///etc/passwd'>]><score-partwise/>";
        let utf16: Vec<u8> = [0xff, 0xfe]
            .into_iter()
            .chain(malicious.encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        assert!(parse_xml(&utf16, "test")
            .unwrap_err()
            .to_string()
            .contains("entity declarations"));
        assert!(parse_xml(
            b"<score-partwise><!-- <!ENTITY harmless> --></score-partwise>",
            "test"
        )
        .is_ok());
    }
    #[test]
    fn preserves_existing_output_without_force() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.xml");
        let output = dir.path().join("output.xml");
        std::fs::write(&input, serialize_xml(&score()).unwrap()).unwrap();
        std::fs::write(&output, "keep").unwrap();
        assert!(write_score(&score(), &output, &input, false)
            .unwrap_err()
            .to_string()
            .contains("already exists"));
        assert_eq!(std::fs::read_to_string(&output).unwrap(), "keep");
        write_score(&score(), &output, &input, true).unwrap();
        assert_eq!(read_score(&output).unwrap().parts[0].name(), "Voice");
    }
    #[test]
    fn preserves_qualified_attributes_and_mixed_text() {
        let root=parse_xml(br##"<score-partwise xmlns:xlink="http://www.w3.org/1999/xlink"><credit><credit-words xml:lang="de" xlink:href="#credit"> First <![CDATA[and]]> last </credit-words></credit></score-partwise>"##,"test").unwrap();
        let original = child(child(&root, "credit").unwrap(), "credit-words").unwrap();
        assert_eq!(original.attributes["xml:lang"], "de");
        assert_eq!(original.attributes["xlink:href"], "#credit");
        let serialized = serialize_xml(&root).unwrap();
        let parsed = parse_xml(&serialized, "roundtrip").unwrap();
        let words = child(child(&parsed, "credit").unwrap(), "credit-words").unwrap();
        assert_eq!(words.attributes["xml:lang"], "de");
        assert_eq!(words.attributes["xlink:href"], "#credit");
        assert_eq!(text(words), " First and last ");
    }
    #[test]
    fn detects_duplicates_hidden_by_zip_name_lookup() {
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        for name in ["one.xml", "two.xml"] {
            archive
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            archive.write_all(b"bad").unwrap();
        }
        let mut bytes = archive.finish().unwrap().into_inner();
        for index in 0..bytes.len() - 7 {
            if &bytes[index..index + 7] == b"two.xml" {
                bytes[index..index + 7].copy_from_slice(b"one.xml");
            }
        }
        assert!(read_score_bytes(&bytes, "test.mxl")
            .unwrap_err()
            .to_string()
            .contains("Duplicate"));
    }
    #[test]
    fn rejects_oversized_member_before_decompressing() {
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        archive
            .start_file("META-INF/container.xml", SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"bad").unwrap();
        let mut bytes = archive.finish().unwrap().into_inner();
        let central = bytes.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
        bytes[central + 24..central + 28]
            .copy_from_slice(&(MAX_XML_BYTES as u32 + 1).to_le_bytes());
        assert!(read_score_bytes(&bytes, "test.mxl")
            .unwrap_err()
            .to_string()
            .contains("64 MiB"));
    }
    #[cfg(unix)]
    #[test]
    fn protects_source_aliases() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.xml");
        let symlink = dir.path().join("symlink.xml");
        let hardlink = dir.path().join("hardlink.xml");
        std::fs::write(&input, serialize_xml(&score()).unwrap()).unwrap();
        std::os::unix::fs::symlink(&input, &symlink).unwrap();
        std::fs::hard_link(&input, &hardlink).unwrap();
        for alias in [symlink, hardlink] {
            assert!(check_output(&input, &alias, true)
                .unwrap_err()
                .to_string()
                .contains("Input replacement"));
        }
    }
}
