//! Full-score MusicXML preparation shared by arranging and comparison.
use crate::{archive::plain_text, renderer, Result, ScoreArchive};
use scorewing_core::xml::{
    child_text, children, children_mut, remove_children, set_child, visit_mut,
};
use scorewing_core::{Element, XMLNode};
use std::{ffi::OsString, fs::File, io::Read, path::Path, sync::atomic::AtomicBool};

mod marks;

pub struct Conversion {
    pub musicxml: Vec<u8>,
    pub warnings: Vec<String>,
}

pub fn convert_musicxml(bytes: &[u8], context: &str, executable: Option<&Path>) -> Result<Vec<u8>> {
    Ok(convert_musicxml_with_warnings(bytes, context, executable)?.musicxml)
}

pub fn convert_musicxml_with_warnings(
    bytes: &[u8],
    context: &str,
    executable: Option<&Path>,
) -> Result<Conversion> {
    let archive = ScoreArchive::read_bytes(bytes, context)?;
    let (export, sources) = export_score(&archive)?;
    let executable = renderer::find_executable(executable)?;
    let cancelled = AtomicBool::new(false);
    renderer::check_version(&executable, &cancelled)?;
    let temporary = tempfile::tempdir().map_err(|error| error.to_string())?;
    let input = temporary.path().join("full-score.mscz");
    let output = temporary.path().join("converted.musicxml");
    export.write(&input)?;
    renderer::run(
        &executable,
        &[
            OsString::from("-o"),
            output.as_os_str().into(),
            input.as_os_str().into(),
        ],
        renderer::CONVERSION_TIMEOUT,
        &cancelled,
    )?;
    let mut converted = Vec::new();
    File::open(&output)
        .map_err(|error| format!("MuseScore did not produce MusicXML: {error}"))?
        .take(scorewing_core::MAX_XML_BYTES as u64 + 1)
        .read_to_end(&mut converted)
        .map_err(|error| format!("Cannot read converted MusicXML: {error}"))?;
    let mut root = scorewing_core::parse_xml(&converted, "converted.musicxml")
        .map_err(|error| format!("Converted MusicXML cannot be arranged: {error}"))?;
    restore_sources(&mut root, &sources)?;
    let warnings = marks::preserve(archive.master(), &mut root)?;
    converted.clear();
    root.write(&mut converted)
        .map_err(|error| error.to_string())?;
    scorewing_core::read_score_bytes(&converted, "converted.musicxml")
        .map_err(|error| format!("Converted MusicXML cannot be arranged: {error}"))?;
    Ok(Conversion {
        musicxml: converted,
        warnings,
    })
}

struct Source {
    marker: String,
    name: String,
    abbreviation: Option<String>,
    staves: Vec<bool>,
}

fn shown(node: &Element) -> bool {
    ["show", "isStaffVisible"]
        .iter()
        .all(|name| !matches!(child_text(node, name).as_deref(), Some("0" | "false")))
}

fn export_score(archive: &ScoreArchive) -> Result<(ScoreArchive, Vec<Source>)> {
    let mut sources = Vec::new();
    let mut ids = std::collections::HashSet::new();
    let master_staves: Vec<_> = children(archive.master(), "Staff").collect();
    for (index, part) in children(archive.master(), "Part").enumerate() {
        let instrument = part.get_child("Instrument");
        let name = instrument
            .and_then(|i| child_text(i, "longName"))
            .filter(|s| !s.trim().is_empty())
            .or_else(|| child_text(part, "trackName"))
            .unwrap_or_else(|| "Unnamed instrument".into());
        let mut staves = Vec::new();
        for staff in children(part, "Staff") {
            let id = staff
                .attributes
                .get("id")
                .cloned()
                .unwrap_or_else(|| (ids.len() + 1).to_string());
            if !ids.insert(id.clone()) {
                return Err("Cannot safely map duplicate MuseScore staff IDs".into());
            }
            let matching: Vec<_> = master_staves
                .iter()
                .filter(|s| s.attributes.get("id") == Some(&id))
                .collect();
            if matching.len() != 1 {
                return Err(
                    "Cannot safely map MuseScore instrument staves to master staves".into(),
                );
            }
            staves.push(shown(part) && shown(staff) && shown(matching[0]));
        }
        if staves.is_empty() {
            return Err("MuseScore instrument has no staves".into());
        }
        sources.push(Source {
            marker: format!("ScorewingSource{:06}", index + 1),
            name: plain_text(&name),
            abbreviation: instrument
                .and_then(|i| child_text(i, "shortName"))
                .map(|s| plain_text(&s)),
            staves,
        });
    }
    if ids.len() != master_staves.len() {
        return Err("Cannot safely map MuseScore instrument staves to master staves".into());
    }
    let mut export = archive.clone();
    export.master_mut().children.retain(|n| {
        n.as_element()
            .is_none_or(|e| !matches!(e.name.as_str(), "Excerpt" | "Score"))
    });
    for (part, source) in children_mut(export.master_mut(), "Part").zip(&sources) {
        set_child(part, "trackName", &source.marker);
        set_child(part, "show", "1");
        if let Some(instrument) = part.get_mut_child("Instrument") {
            // Rich-text children such as <br/> would alter the exported identity marker.
            remove_children(instrument, "longName");
            set_child(instrument, "longName", &source.marker);
        }
        for staff in children_mut(part, "Staff") {
            set_child(staff, "show", "1");
            set_child(staff, "isStaffVisible", "1");
        }
    }
    for staff in children_mut(export.master_mut(), "Staff") {
        set_child(staff, "show", "1");
        set_child(staff, "isStaffVisible", "1");
    }
    visit_mut(export.master_mut(), &mut |node| {
        if matches!(node.name.as_str(), "Tempo" | "RehearsalMark") {
            set_child(node, "visible", "1");
        }
    });
    Ok((export, sources))
}

fn restore_sources(root: &mut Element, sources: &[Source]) -> Result<()> {
    let mismatch = || {
        "Converted MusicXML changed instrument order, identity or staff count; source visibility cannot be mapped safely".to_owned()
    };
    if root.name != "score-partwise" || children(root, "part").count() != sources.len() {
        return Err(mismatch());
    }
    let list = root.get_mut_child("part-list").ok_or_else(mismatch)?;
    if children(list, "score-part").count() != sources.len() {
        return Err(mismatch());
    }
    let mut part_ids = Vec::new();
    for (definition, source) in children_mut(list, "score-part").zip(sources) {
        if child_text(definition, "part-name").as_deref() != Some(&source.marker) {
            return Err(mismatch());
        }
        let id = definition
            .attributes
            .get("id")
            .filter(|s| !s.is_empty())
            .ok_or_else(mismatch)?
            .clone();
        if part_ids.contains(&id) {
            return Err(mismatch());
        }
        part_ids.push(id);
        set_child(definition, "part-name", &source.name);
        if let Some(abbreviation) = &source.abbreviation {
            set_child(definition, "part-abbreviation", abbreviation);
        }
        for instrument in children_mut(definition, "score-instrument") {
            if child_text(instrument, "instrument-name").as_deref() == Some(&source.marker) {
                set_child(instrument, "instrument-name", &source.name);
            }
        }
    }
    for ((part, source), id) in children_mut(root, "part").zip(sources).zip(part_ids) {
        if part.attributes.get("id") != Some(&id) {
            return Err(mismatch());
        }
        let staff_count = children(part, "measure")
            .flat_map(|m| children(m, "attributes"))
            .filter_map(|a| child_text(a, "staves"))
            .map(|v| v.parse::<usize>())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| mismatch())?
            .into_iter()
            .max()
            .unwrap_or(1);
        if staff_count != source.staves.len() {
            return Err(mismatch());
        }
        for measure in children_mut(part, "measure") {
            for attributes in children_mut(measure, "attributes") {
                for details in children_mut(attributes, "staff-details") {
                    details.attributes.remove("print-object");
                }
            }
        }
        let measure = part
            .children
            .iter_mut()
            .filter_map(XMLNode::as_mut_element)
            .find(|n| n.name == "measure")
            .ok_or_else(mismatch)?;
        if measure.get_child("attributes").is_none() {
            measure
                .children
                .insert(0, XMLNode::Element(Element::new("attributes")));
        }
        let attributes = measure.get_mut_child("attributes").unwrap();
        // Source visibility owns this field, including partly hidden multi-staff instruments.
        for (index, visible) in source.staves.iter().enumerate() {
            let number = (index + 1).to_string();
            let existing = children_mut(attributes, "staff-details").find(|d| {
                d.attributes
                    .get("number")
                    .map(String::as_str)
                    .unwrap_or("1")
                    == number
            });
            if let Some(details) = existing {
                details.attributes.insert(
                    "print-object".into(),
                    if *visible { "yes" } else { "no" }.into(),
                );
            } else {
                let mut details = Element::new("staff-details");
                details.attributes.insert("number".into(), number);
                details.attributes.insert(
                    "print-object".into(),
                    if *visible { "yes" } else { "no" }.into(),
                );
                let insertion = attributes
                    .children
                    .iter()
                    .position(|n| {
                        n.as_element().is_some_and(|e| {
                            matches!(
                                e.name.as_str(),
                                "transpose" | "for-part" | "directive" | "measure-style"
                            )
                        })
                    })
                    .unwrap_or(attributes.children.len());
                attributes
                    .children
                    .insert(insertion, XMLNode::Element(details));
            }
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    const SOURCE: &[u8] = br#"<museScore version="4.70"><Score><Division>480</Division>
      <Part id="1"><Staff id="1"/><trackName>Visible</trackName><Instrument id="flute"><longName>Visible<br/></longName><Channel><program value="73"/></Channel></Instrument></Part>
      <Part id="2"><Staff id="2"><isStaffVisible>0</isStaffVisible></Staff><show>0</show><trackName>Hidden</trackName><Instrument id="violin"><longName>Hidden</longName><Channel><program value="40"/></Channel></Instrument></Part>
      <Part id="3"><Staff id="3"/><Staff id="4"><isStaffVisible>0</isStaffVisible></Staff><trackName>Piano</trackName><Instrument id="piano"><longName>Piano</longName><Channel><program value="0"/></Channel></Instrument></Part>
      <Staff id="1"><Measure><voice><TimeSig><sigN>4</sigN><sigD>4</sigD></TimeSig><Chord><durationType>whole</durationType><Note><pitch>60</pitch><tpc>14</tpc></Note><Note><pitch>64</pitch><tpc>18</tpc></Note></Chord></voice></Measure></Staff>
      <Staff id="2"><show>0</show><Measure><voice><TimeSig><sigN>4</sigN><sigD>4</sigD></TimeSig><Chord><durationType>quarter</durationType><Note><pitch>67</pitch><tpc>15</tpc></Note><Note><pitch>71</pitch><tpc>19</tpc></Note></Chord><Tempo><tempo>2</tempo><text>Allegro &lt;sym&gt;metNoteQuarterUp&lt;/sym&gt; = 120</text></Tempo><RehearsalMark><text>A</text></RehearsalMark><Rest><durationType>half</durationType><dots>1</dots></Rest></voice></Measure></Staff>
      <Staff id="3"><Measure><voice><Rest><durationType>measure</durationType><duration>4/4</duration></Rest></voice></Measure></Staff>
      <Staff id="4"><Measure><voice><Rest><durationType>measure</durationType><duration>4/4</duration></Rest></voice></Measure></Staff>
      </Score></museScore>"#;

    fn exported() -> String {
        let definitions = (1..=3).map(|i| format!("<score-part id=\"P{i}\"><part-name>ScorewingSource{i:06}</part-name></score-part>")).collect::<String>();
        let parts = (1..=3).map(|i| format!("<part id=\"P{i}\"><measure number=\"1\"><attributes><divisions>4</divisions>{}</attributes><note><rest/><duration>16</duration><voice>1</voice><type>whole</type></note></measure></part>", if i == 3 { "<staves>2</staves>" } else { "" })).collect::<String>();
        format!("<score-partwise version=\"4.0\"><part-list>{definitions}</part-list>{parts}</score-partwise>")
    }

    fn fake(directory: &Path) -> std::path::PathBuf {
        let executable = directory.join("fake-converter");
        fs::write(&executable, "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'MuseScore Studio 4.7.0'; exit; fi\nprintf '%s' \"$3\" > \"$(dirname \"$0\")/input-path\"\ncp \"$3\" \"$(dirname \"$0\")/prepared.mscz\"\nif [ -f \"$(dirname \"$0\")/fail\" ]; then echo 'conversion failed' >&2; exit 1; fi\ncp \"$(dirname \"$0\")/result.xml\" \"$2\"\n").unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        executable
    }

    #[test]
    fn prepares_formatted_names_and_native_staff_visibility() {
        let archive = ScoreArchive::read_bytes(SOURCE, "source").unwrap();
        let (export, sources) = export_score(&archive).unwrap();
        assert_eq!(sources[1].staves, [false]);
        assert_eq!(sources[2].staves, [true, false]);
        for (part, source) in children(export.master(), "Part").zip(&sources) {
            let name = part
                .get_child("Instrument")
                .unwrap()
                .get_child("longName")
                .unwrap();
            assert!(name.children.iter().all(|node| node.as_element().is_none()));
            assert_eq!(name.get_text().as_deref(), Some(source.marker.as_str()));
            for staff in children(part, "Staff") {
                assert_eq!(child_text(staff, "isStaffVisible").as_deref(), Some("1"));
            }
        }
        assert_eq!(
            child_text(
                children(archive.master(), "Part")
                    .nth(1)
                    .unwrap()
                    .get_child("Staff")
                    .unwrap(),
                "isStaffVisible"
            )
            .as_deref(),
            Some("0")
        );
    }

    #[test]
    fn converts_all_staves_restores_visibility_and_recovers_hidden_marks() {
        let directory = tempfile::tempdir().unwrap();
        let executable = fake(directory.path());
        fs::write(directory.path().join("result.xml"), exported()).unwrap();
        let original = ScoreArchive::read_bytes(SOURCE, "score.mscx").unwrap();
        let compressed = directory.path().join("source.mscz");
        original.write(&compressed).unwrap();
        for bytes in [SOURCE.to_vec(), fs::read(compressed).unwrap()] {
            let converted =
                convert_musicxml_with_warnings(&bytes, "source", Some(&executable)).unwrap();
            assert!(converted.warnings.is_empty(), "{:?}", converted.warnings);
            let score = scorewing_core::read_score_bytes(&converted.musicxml, "converted").unwrap();
            assert_eq!(score.parts.len(), 3);
            assert_eq!(
                score.parts.iter().map(|p| p.name()).collect::<Vec<_>>(),
                ["Visible", "Hidden", "Piano"]
            );
            let root = scorewing_core::parse_xml(&converted.musicxml, "converted").unwrap();
            scorewing_core::validate_score(&root).unwrap();
            let staves = children(&root, "part")
                .flat_map(|p| children(p, "measure"))
                .flat_map(|m| children(m, "attributes"))
                .flat_map(|a| children(a, "staff-details"))
                .map(|s| s.attributes["print-object"].as_str())
                .collect::<Vec<_>>();
            assert_eq!(staves, ["yes", "no", "yes", "no"]);
            let first = children(&root, "part").next().unwrap();
            let directions =
                children(first.get_child("measure").unwrap(), "direction").collect::<Vec<_>>();
            assert_eq!(directions.len(), 4);
            assert!(directions
                .iter()
                .all(|d| child_text(d, "offset").as_deref() == Some("4")));
            assert_eq!(
                directions
                    .iter()
                    .filter_map(|d| d.get_child("sound"))
                    .next()
                    .unwrap()
                    .attributes["tempo"],
                "120"
            );
            let prepared = ScoreArchive::read_bytes(
                &fs::read(directory.path().join("prepared.mscz")).unwrap(),
                "prepared",
            )
            .unwrap();
            assert_eq!(children(prepared.master(), "Staff").count(), 4);
            assert!(children(prepared.master(), "Part").all(shown));
            assert!(children(prepared.master(), "Staff").all(shown));
            assert!(!original.parts().unwrap()[1].visible);
            assert!(
                !Path::new(&fs::read_to_string(directory.path().join("input-path")).unwrap())
                    .exists()
            );
        }
    }

    #[test]
    fn rejects_failed_invalid_changed_and_oversized_exports_and_cleans_up() {
        let directory = tempfile::tempdir().unwrap();
        let executable = fake(directory.path());
        fs::write(directory.path().join("fail"), "").unwrap();
        assert!(convert_musicxml(SOURCE, "score", Some(&executable))
            .unwrap_err()
            .contains("conversion failed"));
        fs::remove_file(directory.path().join("fail")).unwrap();
        for result in [
            "invalid".into(),
            exported().replace("ScorewingSource000002", "ScorewingSource000001"),
            exported().replace("<staves>2</staves>", ""),
            " ".repeat(scorewing_core::MAX_XML_BYTES + 1),
        ] {
            fs::write(directory.path().join("result.xml"), result).unwrap();
            assert!(convert_musicxml(SOURCE, "score", Some(&executable)).is_err());
            assert!(
                !Path::new(&fs::read_to_string(directory.path().join("input-path")).unwrap())
                    .exists()
            );
        }
    }

    #[test]
    fn visibility_precedes_transpose_and_preserves_existing_staff_details() {
        let archive = ScoreArchive::read_bytes(SOURCE, "source").unwrap();
        let (_, sources) = export_score(&archive).unwrap();
        let xml = exported().replace("</attributes>", "<staff-details><staff-lines>5</staff-lines></staff-details><transpose><diatonic>-1</diatonic><chromatic>-2</chromatic></transpose></attributes>");
        let mut root = scorewing_core::parse_xml(xml.as_bytes(), "export").unwrap();
        restore_sources(&mut root, &sources).unwrap();
        scorewing_core::validate_score(&root).unwrap();
        for part in children(&root, "part") {
            let attributes = part
                .get_child("measure")
                .unwrap()
                .get_child("attributes")
                .unwrap();
            assert_eq!(
                child_text(
                    attributes.get_child("staff-details").unwrap(),
                    "staff-lines"
                )
                .as_deref(),
                Some("5")
            );
        }
        // A transposing single-staff instrument need not have existing staff-details.
        let xml = exported().replace(
            "</attributes>",
            "<transpose><diatonic>-1</diatonic><chromatic>-2</chromatic></transpose></attributes>",
        );
        let mut root = scorewing_core::parse_xml(xml.as_bytes(), "export").unwrap();
        restore_sources(&mut root, &sources).unwrap();
        scorewing_core::validate_score(&root).unwrap();
    }

    #[test]
    fn warns_when_native_mark_position_cannot_be_verified() {
        let directory = tempfile::tempdir().unwrap();
        let executable = fake(directory.path());
        fs::write(directory.path().join("result.xml"), exported()).unwrap();
        let source = String::from_utf8(SOURCE.to_vec()).unwrap().replace(
            "<durationType>quarter</durationType>",
            "<durationType>unsupported</durationType>",
        );
        let converted =
            convert_musicxml_with_warnings(source.as_bytes(), "source", Some(&executable)).unwrap();
        assert_eq!(converted.warnings.len(), 2);
        assert!(converted
            .warnings
            .iter()
            .all(|w| w.contains("measure 1, beat unknown")));
        assert!(converted.warnings[0].contains("Allegro"));
        assert!(converted.warnings[1].contains("RehearsalMark 'A'"));
    }

    #[test]
    #[ignore = "requires MUSESCORE_EXECUTABLE pointing to supported MuseScore 4.7"]
    fn native_full_score_conversion_preserves_hidden_multistaff_and_marks() {
        let executable = std::env::var_os("MUSESCORE_EXECUTABLE").expect("MUSESCORE_EXECUTABLE");
        let result =
            convert_musicxml_with_warnings(SOURCE, "native.mscx", Some(Path::new(&executable)))
                .unwrap();
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        let score = scorewing_core::read_score_bytes(&result.musicxml, "native").unwrap();
        assert_eq!(score.parts.len(), 3);
        assert_eq!(score.parts[2].staff_count(), 2);
        assert!(!score.parts[1].visible());
        let root = scorewing_core::parse_xml(&result.musicxml, "native").unwrap();
        scorewing_core::validate_score(&root).unwrap();
        for part in &score.parts {
            for extra in part.measures.iter().flat_map(|m| &m.extras) {
                let mut required = false;
                scorewing_core::xml::visit(&extra.element, &mut |n| {
                    required |= n.name == "rehearsal"
                        || n.name == "metronome"
                        || (n.name == "sound" && n.attributes.contains_key("tempo"));
                });
                if required {
                    let offset = child_text(&extra.element, "offset")
                        .unwrap_or_else(|| "0".into())
                        .parse::<i64>()
                        .unwrap();
                    assert_eq!(
                        &extra.position + scorewing_core::frac(offset) / &extra.divisions,
                        scorewing_core::frac(1)
                    );
                }
            }
        }
        let mut tempo = false;
        let mut rehearsal = false;
        scorewing_core::xml::visit(&root, &mut |n| {
            tempo |= n.name == "sound" && n.attributes.get("tempo").is_some_and(|v| v == "120");
            rehearsal |= n.name == "rehearsal" && n.get_text().as_deref() == Some("A");
        });
        assert!(tempo && rehearsal);
        let visible = score.parts[0].id().to_owned();
        let hidden = score.parts[1].id().to_owned();
        for generated in [
            scorewing_core::split_score(&score, "chords", Some(&[visible.clone(), hidden.clone()]))
                .unwrap()
                .root,
            scorewing_core::merge_score(&score, &[vec![visible, hidden]]).unwrap(),
        ] {
            scorewing_core::validate_score(&generated).unwrap();
            let arranged = scorewing_core::parse_score(generated).unwrap();
            assert!(arranged.parts.len() > 3);
            for original in &score.parts {
                let retained = arranged
                    .parts
                    .iter()
                    .find(|p| p.id() == original.id())
                    .unwrap();
                assert_eq!(retained.source, original.source);
                assert_eq!(retained.visible(), original.visible());
            }
            for new in arranged
                .parts
                .iter()
                .filter(|p| !score.parts.iter().any(|old| old.id() == p.id()))
            {
                assert!(new.visible());
                let mut tempos = 0;
                let mut rehearsals = 0;
                scorewing_core::xml::visit(&new.source, &mut |n| {
                    tempos += usize::from(n.name == "sound" && n.attributes.contains_key("tempo"));
                    rehearsals +=
                        usize::from(n.name == "rehearsal" && n.get_text().as_deref() == Some("A"));
                });
                assert_eq!(tempos, 1);
                assert_eq!(rehearsals, 1);
            }
        }
    }
}
