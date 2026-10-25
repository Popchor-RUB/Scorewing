use crate::{PracticeOptions, PracticePart, Preset, PresetId, Result, SoundCatalog};
use mxml_core::xml::{child_text, children, elements, set_child, visit, visit_mut};
use mxml_core::{parse_xml, Element, XMLNode};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::sync::Arc;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

const MEMBER_LIMIT: usize = 64 * 1024 * 1024;
const TOTAL_LIMIT: usize = 256 * 1024 * 1024;
const ENTRY_LIMIT: usize = 4096;

#[derive(Clone, Debug)]
pub struct ScoreArchive {
    root: Element,
    score_name: String,
    entries: BTreeMap<String, Arc<[u8]>>,
    audio: Value,
}

impl ScoreArchive {
    pub fn read_bytes(bytes: &[u8], context: &str) -> Result<Self> {
        if bytes.len() > MEMBER_LIMIT {
            return Err("Input exceeds the 64 MiB limit".into());
        }
        let mut entries = BTreeMap::new();
        let score_name;
        let root;
        if bytes.starts_with(b"PK") {
            let entry_count = validate_zip_directory(bytes)?;
            let mut zip = ZipArchive::new(Cursor::new(bytes))
                .map_err(|e| format!("Invalid MuseScore archive: {e}"))?;
            if zip.len() != entry_count {
                return Err("Duplicate or ambiguous archive member names".into());
            }
            if zip.len() > ENTRY_LIMIT {
                return Err("Archive exceeds 4096 entries".into());
            }
            let mut total = 0usize;
            for index in 0..zip.len() {
                let mut member = zip
                    .by_index(index)
                    .map_err(|e| format!("Cannot read archive member: {e}"))?;
                let name = member.name().to_owned();
                safe_name(&name)?;
                if entries.contains_key(&name) {
                    return Err(format!("Duplicate archive member: {name}"));
                }
                if member.size() > MEMBER_LIMIT as u64 {
                    return Err(format!("Archive member exceeds 64 MiB: {name}"));
                }
                total = total
                    .checked_add(member.size() as usize)
                    .ok_or("Archive size overflow")?;
                if total > TOTAL_LIMIT {
                    return Err("Archive exceeds the 256 MiB decompressed limit".into());
                }
                let mut data = Vec::new();
                member
                    .by_ref()
                    .take(MEMBER_LIMIT as u64 + 1)
                    .read_to_end(&mut data)
                    .map_err(|e| e.to_string())?;
                if data.len() != member.size() as usize {
                    return Err(format!("Invalid archive member size: {name}"));
                }
                entries.insert(name, Arc::from(data));
            }
            let container = entries
                .get("META-INF/container.xml")
                .ok_or("Score archive has no META-INF/container.xml")?;
            let metadata = parse_xml(container, context).map_err(|e| e.to_string())?;
            let mut candidates = Vec::new();
            visit(&metadata, &mut |node| {
                if node.name == "rootfile" {
                    if let Some(name) = node.attributes.get("full-path") {
                        if name.to_ascii_lowercase().ends_with(".mscx") {
                            candidates.push(name.clone());
                        }
                    }
                }
            });
            if candidates.len() != 1 {
                return Err("Archive metadata must identify one master score XML file".into());
            }
            score_name = candidates.remove(0);
            safe_name(&score_name)?;
            root = parse_xml(
                entries
                    .get(&score_name)
                    .ok_or("Archive master score is missing")?,
                context,
            )
            .map_err(|e| e.to_string())?;
        } else {
            root = parse_xml(bytes, context).map_err(|e| e.to_string())?;
            score_name = "score.mscx".into();
        }
        if root.name != "museScore"
            || !root
                .attributes
                .get("version")
                .is_some_and(|v| v.starts_with("4."))
        {
            return Err(
                "Only MuseScore 4 scores are supported. Resave this file in MuseScore 4.".into(),
            );
        }
        if children(&root, "Score").count() != 1 {
            return Err("Score must contain one master Score element".into());
        }
        let audio: Value = match entries.get("audiosettings.json") {
            Some(data) => {
                serde_json::from_slice(data).map_err(|e| format!("Invalid audio settings: {e}"))?
            }
            None => json!({}),
        };
        validate_audio(&audio)?;
        let archive = Self {
            root,
            score_name,
            entries,
            audio,
        };
        if archive.parts()?.is_empty() {
            return Err("Score has no instruments".into());
        }
        Ok(archive)
    }

    fn master(&self) -> &Element {
        self.root.get_child("Score").unwrap()
    }
    fn master_mut(&mut self) -> &mut Element {
        self.root.get_mut_child("Score").unwrap()
    }

    /// Remove explicitly hidden music before export; layout-based hiding is irrelevant.
    pub(crate) fn visible_score(&self) -> Result<Self> {
        let shown =
            |node: &Element| !matches!(child_text(node, "show").as_deref(), Some("0" | "false"));
        let mut score = self.clone();
        let mut ordinal = 0usize;
        let mut retained = BTreeMap::new();
        let mut known = HashSet::new();
        for part in children(self.master(), "Part") {
            for staff in children(part, "Staff") {
                ordinal += 1;
                let id = staff
                    .attributes
                    .get("id")
                    .cloned()
                    .unwrap_or_else(|| ordinal.to_string());
                if id.parse::<usize>().ok() != Some(ordinal) || !known.insert(id.clone()) {
                    return Err(
                        "Cannot safely convert non-sequential or duplicate MuseScore staff IDs"
                            .into(),
                    );
                }
                let master_visible = children(self.master(), "Staff")
                    .find(|master| master.attributes.get("id") == Some(&id))
                    .is_none_or(shown);
                if shown(part) && shown(staff) && master_visible {
                    retained.insert(id, (retained.len() + 1).to_string());
                }
            }
        }
        if retained.is_empty() {
            return Err("The MuseScore score has no visible instruments or staves.".into());
        }
        let master_ids: Vec<_> = children(self.master(), "Staff")
            .filter_map(|staff| staff.attributes.get("id"))
            .collect();
        if master_ids.len() != known.len()
            || master_ids.iter().any(|id| !known.contains(*id))
            || master_ids.iter().collect::<HashSet<_>>().len() != known.len()
        {
            return Err("Cannot safely map MuseScore instrument staves to master staves".into());
        }
        let removing = retained.len() != known.len();
        let mut next_staff = 0usize;
        score.master_mut().children.retain_mut(|node| {
            let Some(node) = node.as_mut_element() else {
                return true;
            };
            if node.name == "Excerpt" || node.name == "Score" {
                return false;
            }
            if node.name == "Part" {
                node.children.retain_mut(|child| {
                    let Some(staff) = child.as_mut_element() else {
                        return true;
                    };
                    if staff.name != "Staff" {
                        return true;
                    }
                    next_staff += 1;
                    let id = staff
                        .attributes
                        .get("id")
                        .cloned()
                        .unwrap_or_else(|| next_staff.to_string());
                    if let Some(new) = retained.get(&id) {
                        staff.attributes.insert("id".into(), new.clone());
                        // Brackets and barline spans describe layout across the original staff set.
                        if removing {
                            staff.children.retain(|n| {
                                !n.as_element().is_some_and(|e| {
                                    matches!(e.name.as_str(), "bracket" | "barLineSpan")
                                })
                            });
                        }
                        true
                    } else {
                        false
                    }
                });
                return children(node, "Staff").next().is_some();
            }
            if node.name == "Staff" {
                let Some(new) = node.attributes.get("id").and_then(|id| retained.get(id)) else {
                    return false;
                };
                node.attributes.insert("id".into(), new.clone());
            }
            true
        });
        if removing {
            let mut error = None;
            visit_mut(score.master_mut(), &mut |node| {
                if node
                    .attributes
                    .keys()
                    .any(|key| matches!(key.as_str(), "track" | "track2" | "staff" | "staffId"))
                {
                    error = Some("Cannot remove hidden staves with explicit staff or track attribute references. Simplify these references in a copy first.".into());
                }
                if matches!(
                    node.name.as_str(),
                    "linkedTo" | "linkedMain" | "linked" | "Link"
                ) || (matches!(node.name.as_str(), "staffMove" | "staves")
                    && node.get_text().as_deref() != Some("0"))
                {
                    error = Some("Cannot remove hidden staves from linked or cross-staff notation. Unlink the staves or remove cross-staff notation in a copy first.".to_string());
                }
                if matches!(node.name.as_str(), "track" | "track2") {
                    if let Some(track) = node.get_text().and_then(|text| text.parse::<usize>().ok())
                    {
                        let old = (track / 4 + 1).to_string();
                        if let Some(new) = retained.get(&old) {
                            node.children = vec![XMLNode::Text(
                                ((new.parse::<usize>().unwrap() - 1) * 4 + track % 4).to_string(),
                            )];
                        } else {
                            error = Some("Visible notation references a hidden staff; conversion would discard music.".into());
                        }
                    } else {
                        error = Some("Cannot safely map a MuseScore track reference".into());
                    }
                }
            });
            if let Some(error) = error {
                return Err(error);
            }
        }
        score.audio = json!({});
        Ok(score)
    }

    pub fn parts(&self) -> Result<Vec<PracticePart>> {
        children(self.master(), "Part")
            .enumerate()
            .map(|(index, node)| {
                let name = child_text(node, "trackName")
                    .filter(|s| !s.trim().is_empty())
                    .or_else(|| {
                        node.get_child("Instrument")
                            .and_then(|i| child_text(i, "longName"))
                    })
                    .unwrap_or_else(|| format!("Instrument {}", index + 1));
                let original_instruments = children(node, "Instrument")
                    .map(|i| instrument_id(i).unwrap_or("unknown").to_owned())
                    .collect();
                let visible = !matches!(child_text(node, "show").as_deref(), Some("0" | "false"));
                Ok(PracticePart {
                    key: format!("part-{}", index + 1),
                    name: plain_text(&name),
                    visible,
                    original_instruments,
                    existing_preset: self.existing_preset(node),
                })
            })
            .collect()
    }

    fn existing_preset(&self, part: &Element) -> Option<PresetId> {
        let audio_tracks: Vec<_> = self
            .audio
            .get("tracks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|t| {
                t.get("partId").and_then(Value::as_str)
                    == part.attributes.get("id").map(String::as_str)
            })
            .collect();
        if !audio_tracks.is_empty() {
            let mut presets = audio_tracks.iter().map(|t| {
                let resource = t.get("in")?.get("resourceMeta")?;
                let attrs = resource.get("attributes")?;
                if resource.get("type")?.as_str()? != "fluid_soundfont"
                    || attrs.get("soundFontName")?.as_str()? != "MS Basic"
                {
                    return None;
                }
                let bank = attrs.get("presetBank")?.as_str()?.parse().ok()?;
                let program = attrs.get("presetProgram")?.as_str()?.parse().ok()?;
                Some(PresetId { bank, program })
            });
            let first = presets.next()??;
            return (first.bank <= 16383
                && first.program <= 127
                && presets.all(|p| p == Some(first)))
            .then_some(first);
        }
        let mut presets = children(part, "Instrument")
            .flat_map(|i| children(i, "Channel"))
            .map(|channel| {
                if child_text(channel, "synti")
                    .as_deref()
                    .is_some_and(|s| s != "Fluid")
                {
                    return None;
                }
                let program: u8 = channel
                    .get_child("program")?
                    .attributes
                    .get("value")?
                    .parse()
                    .ok()?;
                let mut high = 0u16;
                let mut low = 0u16;
                for controller in children(channel, "controller") {
                    match controller.attributes.get("ctrl").map(String::as_str) {
                        Some("0") => high = controller.attributes.get("value")?.parse().ok()?,
                        Some("32") => low = controller.attributes.get("value")?.parse().ok()?,
                        _ => (),
                    }
                }
                (high <= 127 && low <= 127 && program <= 127).then_some(PresetId {
                    bank: high * 128 + low,
                    program,
                })
            });
        let first = presets.next()??;
        presets.all(|p| p == Some(first)).then_some(first)
    }

    pub fn verify_normalized(&self, original: &Self) -> Result<()> {
        let before = original.parts()?;
        let after = self.parts()?;
        let before_nodes: Vec<_> = children(original.master(), "Part").collect();
        let after_nodes: Vec<_> = children(self.master(), "Part").collect();
        if before.len() != after.len() {
            return Err("Normalization changed the number of instruments".into());
        }
        let mut ids = HashSet::new();
        for ((old, new), (old_node, new_node)) in before
            .iter()
            .zip(&after)
            .zip(before_nodes.iter().zip(&after_nodes))
        {
            if old.name != new.name
                || canonical_instruments(old_node) != canonical_instruments(new_node)
                || children(old_node, "Staff").count() != children(new_node, "Staff").count()
            {
                return Err("Normalization changed instrument order or identity; settings cannot be mapped safely".into());
            }
            let id = new_node
                .attributes
                .get("id")
                .filter(|s| !s.is_empty())
                .ok_or("Normalized score has missing part IDs")?;
            if !ids.insert(id) {
                return Err("Normalized score has duplicate part IDs".into());
            }
        }
        // Identical parts need preserved IDs to prove they did not exchange their places.
        for (index, old) in before.iter().enumerate() {
            if before
                .iter()
                .enumerate()
                .filter(|(other_index, p)| {
                    p.name == old.name
                        && canonical_instruments(before_nodes[*other_index])
                            == canonical_instruments(before_nodes[index])
                })
                .count()
                > 1
            {
                let old_id = before_nodes[index].attributes.get("id");
                if old_id.is_none() || old_id != after_nodes[index].attributes.get("id") {
                    return Err(
                        "Normalization cannot safely distinguish duplicate instruments".into(),
                    );
                }
            }
        }
        Ok(())
    }

    pub fn merge_assets_from(&mut self, original: &Self) {
        for (name, data) in &original.entries {
            if name != &original.score_name
                && name != "audiosettings.json"
                && name != "META-INF/container.xml"
            {
                self.entries
                    .entry(name.clone())
                    .or_insert_with(|| data.clone());
            }
        }
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        let mut entries = self.entries.clone();
        let mut xml = Vec::new();
        self.root.write(&mut xml).map_err(|e| e.to_string())?;
        entries.insert(self.score_name.clone(), Arc::from(xml));
        entries.insert(
            "audiosettings.json".into(),
            Arc::from(serde_json::to_vec(&self.audio).map_err(|e| e.to_string())?),
        );
        if !entries.contains_key("META-INF/container.xml") {
            let mut container = Element::new("container");
            let mut roots = Element::new("rootfiles");
            let mut root = Element::new("rootfile");
            root.attributes
                .insert("full-path".into(), self.score_name.clone());
            roots.children.push(XMLNode::Element(root));
            container.children.push(XMLNode::Element(roots));
            let mut data = Vec::new();
            container.write(&mut data).map_err(|e| e.to_string())?;
            entries.insert("META-INF/container.xml".into(), Arc::from(data));
        }
        if entries.len() > ENTRY_LIMIT
            || entries.values().any(|v| v.len() > MEMBER_LIMIT)
            || entries.values().map(|v| v.len()).sum::<usize>() > TOTAL_LIMIT
        {
            return Err("Prepared archive exceeds safety limits".into());
        }
        let mut writer = ZipWriter::new(File::create(path).map_err(|e| e.to_string())?);
        for (name, data) in entries {
            writer
                .start_file(
                    name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
                )
                .map_err(|e| e.to_string())?;
            writer.write_all(&data).map_err(|e| e.to_string())?;
        }
        writer.finish().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn mix(
        &self,
        options: &PracticeOptions,
        target_key: &str,
        catalog: &SoundCatalog,
    ) -> Result<Self> {
        let parts = self.parts()?;
        options.validate(&parts, catalog)?;
        if !options
            .parts
            .iter()
            .any(|p| p.key == target_key && p.create_track)
        {
            return Err("Practice target was not selected".into());
        }
        let mut score = self.clone();
        visit_mut(score.master_mut(), &mut flatten);
        let mut tracks = Vec::new();
        let mut known = HashSet::new();
        let mut ids = HashSet::new();
        let mut staff_owners = BTreeMap::new();
        let mut staff_ordinal = 0;
        for (index, part) in children(score.master(), "Part").enumerate() {
            let id = part
                .attributes
                .get("id")
                .filter(|s| !s.is_empty())
                .ok_or("Normalized score has missing part IDs")?
                .clone();
            if !ids.insert(id.clone()) {
                return Err("Normalized score has duplicate part IDs".into());
            }
            let key = format!("part-{}", index + 1);
            if children(part, "Instrument").next().is_none() {
                return Err("Normalized part has no instrument definition".into());
            }
            for staff in children(part, "Staff") {
                staff_ordinal += 1;
                // MuseScore 4.7 omits Part/Staff IDs; its master Staff IDs use global order.
                let staff_id = staff
                    .attributes
                    .get("id")
                    .cloned()
                    .unwrap_or_else(|| staff_ordinal.to_string());
                if staff_owners
                    .insert(staff_id, (key.clone(), id.clone()))
                    .is_some()
                {
                    return Err("Duplicate staff ownership".into());
                }
            }
        }
        let mut master_staff_ids = HashSet::new();
        for staff in children(score.master(), "Staff") {
            let id = staff
                .attributes
                .get("id")
                .ok_or("Normalized master staff has no ID")?;
            if !staff_owners.contains_key(id) || !master_staff_ids.insert(id.clone()) {
                return Err("Normalized score has unknown or duplicate master staff IDs".into());
            }
        }
        if master_staff_ids.len() != staff_owners.len() {
            return Err("Normalized score has missing master staves".into());
        }
        let mut failure = None;
        for (index, part) in score
            .master_mut()
            .children
            .iter_mut()
            .filter_map(XMLNode::as_mut_element)
            .filter(|e| e.name == "Part")
            .enumerate()
        {
            let key = format!("part-{}", index + 1);
            let setting = options.parts.iter().find(|p| p.key == key).unwrap();
            let active = key == target_key || setting.include_in_background;
            if active {
                set_child(part, "show", "1");
                for staff in part
                    .children
                    .iter_mut()
                    .filter_map(XMLNode::as_mut_element)
                    .filter(|e| e.name == "Staff")
                {
                    set_child(staff, "show", "1");
                }
            }
            let id = part.attributes.get("id").unwrap().clone();
            for instrument in part
                .children
                .iter_mut()
                .filter_map(XMLNode::as_mut_element)
                .filter(|e| e.name == "Instrument")
            {
                if let Err(e) = configure(
                    instrument,
                    &id,
                    &key,
                    target_key,
                    options,
                    catalog,
                    &mut tracks,
                    &mut known,
                ) {
                    failure = Some(e);
                }
            }
        }
        for staff in score
            .master_mut()
            .children
            .iter_mut()
            .filter_map(XMLNode::as_mut_element)
            .filter(|e| e.name == "Staff")
        {
            let owner = staff
                .attributes
                .get("id")
                .and_then(|id| staff_owners.get(id));
            if let Some((key, id)) = owner {
                let active = key == target_key
                    || options
                        .parts
                        .iter()
                        .find(|setting| setting.key == *key)
                        .unwrap()
                        .include_in_background;
                visit_mut(staff, &mut |node| {
                    // Native headless export ignores persisted mixer mute; exclude note playback directly.
                    if node.name == "Note" && !active {
                        set_child(node, "play", "0");
                    }
                    if node.name == "Instrument" {
                        if let Err(e) = configure(
                            node,
                            id,
                            key,
                            target_key,
                            options,
                            catalog,
                            &mut tracks,
                            &mut known,
                        ) {
                            failure = Some(e);
                        }
                    }
                });
            }
        }
        if let Some(e) = failure {
            return Err(e);
        }
        let mut native_metronome = false;
        for old in self
            .audio
            .get("tracks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let part_id = old.get("partId").and_then(Value::as_str).unwrap();
            let instrument_id = old.get("instrumentId").and_then(Value::as_str).unwrap();
            if known.contains(&(part_id.to_owned(), instrument_id.to_owned())) {
                continue;
            }
            let metronome = part_id == "999" && instrument_id == "metronome";
            if metronome && native_metronome {
                return Err("Duplicate native metronome tracks".into());
            }
            native_metronome |= metronome;
            let mut track = old.clone();
            track["out"] = output(if metronome { options.metronome_db } else { 0.0 });
            track["soloMuteState"] =
                json!({"mute": !metronome || !options.metronome_enabled, "solo":false});
            tracks.push(track);
        }
        if !native_metronome {
            // An empty input asks MuseScore to resolve its own native metronome resource.
            tracks.push(json!({"partId":"999","instrumentId":"metronome","in":{},"out":output(options.metronome_db),
                "soloMuteState":{"mute":!options.metronome_enabled,"solo":false}}));
        }
        score.audio = json!({"activeSoundProfile":"MuseScore Basic", "master":output(0.0),"aux":[],"tracks":tracks});
        Ok(score)
    }
}

fn validate_zip_directory(bytes: &[u8]) -> Result<usize> {
    // zip indexes entries by name and silently drops duplicates, so validate raw headers first.
    let eocd = (bytes.len().saturating_sub(65_557)..bytes.len().saturating_sub(21))
        .rev()
        .find(|&p| {
            &bytes[p..p + 4] == b"PK\x05\x06"
                && p + 22 + u16::from_le_bytes([bytes[p + 20], bytes[p + 21]]) as usize
                    == bytes.len()
        })
        .ok_or("Invalid ZIP end-of-directory record")?;
    let short = |p| u16::from_le_bytes([bytes[p], bytes[p + 1]]) as usize;
    let long = |p| u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap()) as usize;
    let count = short(eocd + 10);
    if count > ENTRY_LIMIT {
        return Err("Archive exceeds 4096 entries".into());
    }
    if short(eocd + 4) != 0 || short(eocd + 6) != 0 || short(eocd + 8) != count {
        return Err("Multi-disk MuseScore archives are unsupported".into());
    }
    let mut position = long(eocd + 16);
    let end = position
        .checked_add(long(eocd + 12))
        .filter(|end| *end == eocd)
        .ok_or("Invalid ZIP central-directory bounds")?;
    let mut names = HashSet::new();
    for _ in 0..count {
        if position.checked_add(46).is_none_or(|p| p > end)
            || &bytes[position..position + 4] != b"PK\x01\x02"
        {
            return Err("Invalid ZIP central-directory entry".into());
        }
        let name_end = position + 46 + short(position + 28);
        let next = name_end
            .checked_add(short(position + 30) + short(position + 32))
            .filter(|p| *p <= end)
            .ok_or("Invalid ZIP entry bounds")?;
        if !names.insert(&bytes[position + 46..name_end]) {
            return Err("Duplicate archive member".into());
        }
        position = next;
    }
    if position != end {
        return Err("ZIP entry count does not match its directory".into());
    }
    Ok(count)
}

impl PracticeOptions {
    pub fn validate(&self, parts: &[PracticePart], catalog: &SoundCatalog) -> Result<()> {
        if !self.background_reduction_db.is_finite()
            || !(0.0..=60.0).contains(&self.background_reduction_db)
            || !self.metronome_db.is_finite()
            || !(-60.0..=0.0).contains(&self.metronome_db)
        {
            return Err("Background reduction must be 0–60 dB and metronome level −60–0 dB".into());
        }
        if self.parts.len() != parts.len() {
            return Err("Settings must include every score instrument exactly once".into());
        }
        let mut seen = HashSet::new();
        for part in &self.parts {
            if !parts.iter().any(|p| p.key == part.key) || !seen.insert(&part.key) {
                return Err("Unknown or duplicate instrument key".into());
            }
            for (required, sound) in [
                (part.create_track, part.foreground_sound),
                (part.include_in_background, part.background_sound),
            ] {
                if required {
                    catalog
                        .get(sound.ok_or_else(|| {
                            format!("Select a sound for instrument {}", part.key)
                        })?)?;
                }
            }
        }
        if !self.parts.iter().any(|p| p.create_track) {
            return Err("Select at least one practice instrument".into());
        }
        Ok(())
    }
}

fn safe_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.starts_with('/')
        || name.contains(['\\', '\0', ':'])
        || name
            .trim_end_matches('/')
            .split('/')
            .any(|s| s == ".." || s == "." || s.is_empty())
        || name.ends_with("//")
    {
        return Err(format!("Unsafe archive member path: {name}"));
    }
    Ok(())
}

fn validate_audio(audio: &Value) -> Result<()> {
    if !audio.is_object() {
        return Err("Audio settings must be a JSON object".into());
    }
    for name in ["master"] {
        if audio.get(name).is_some_and(|v| !v.is_object()) {
            return Err(format!("Audio {name} must be an object"));
        }
    }
    if audio.get("aux").is_some_and(|v| !v.is_array()) {
        return Err("Audio aux must be an array".into());
    }
    if let Some(tracks) = audio.get("tracks") {
        let tracks = tracks.as_array().ok_or("Audio tracks must be an array")?;
        if tracks.len() > 16384 {
            return Err("Too many audio tracks".into());
        }
        let mut keys = HashSet::new();
        for track in tracks {
            if !track.is_object() {
                return Err("Audio track must be an object".into());
            }
            for name in ["partId", "instrumentId"] {
                if track
                    .get(name)
                    .and_then(Value::as_str)
                    .is_none_or(|s| s.is_empty())
                {
                    return Err(format!("Audio track {name} must be a nonempty string"));
                }
            }
            for name in ["in", "out", "soloMuteState"] {
                if track.get(name).is_some_and(|v| !v.is_object()) {
                    return Err(format!("Audio track {name} must be an object"));
                }
            }
            if !keys.insert((
                track["partId"].as_str().unwrap(),
                track["instrumentId"].as_str().unwrap(),
            )) {
                return Err("Duplicate audio track identifier".into());
            }
            if let Some(state) = track.get("soloMuteState") {
                for name in ["mute", "solo"] {
                    if state.get(name).is_some_and(|v| !v.is_boolean()) {
                        return Err(format!("Audio {name} must be a boolean"));
                    }
                }
            }
            if let Some(out) = track.get("out") {
                for name in ["volumeDb", "balance"] {
                    if out.get(name).is_some_and(|v| !v.is_number()) {
                        return Err(format!("Audio {name} must be a number"));
                    }
                }
                if out.get("auxSends").is_some_and(|v| !v.is_array())
                    || out.get("fxChain").is_some_and(|v| !v.is_object())
                {
                    return Err("Invalid audio output settings".into());
                }
            }
            if let Some(input) = track.get("in") {
                for name in ["resourceMeta", "unitConfiguration"] {
                    if input.get(name).is_some_and(|v| !v.is_object()) {
                        return Err(format!("Audio input {name} must be an object"));
                    }
                }
                if input
                    .get("resourceMeta")
                    .and_then(|v| v.get("attributes"))
                    .is_some_and(|v| !v.is_object())
                {
                    return Err("Audio resource attributes must be an object".into());
                }
                if let Some(resource) = input.get("resourceMeta") {
                    for name in ["id", "type", "vendor"] {
                        if resource.get(name).is_some_and(|v| !v.is_string()) {
                            return Err(format!("Audio resource {name} must be a string"));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn plain_text(text: &str) -> String {
    let wrapped = format!("<text>{text}</text>");
    if let Ok(root) = parse_xml(wrapped.as_bytes(), "instrument name") {
        fn collect(node: &Element, result: &mut String) {
            for child in &node.children {
                match child {
                    XMLNode::Text(t) | XMLNode::CData(t) => result.push_str(t),
                    XMLNode::Element(e) => collect(e, result),
                    _ => (),
                }
            }
        }
        let mut result = String::new();
        collect(&root, &mut result);
        result
    } else {
        text.into()
    }
}

fn instrument_id(instrument: &Element) -> Option<&str> {
    instrument
        .attributes
        .get("id")
        .map(String::as_str)
        .filter(|s| !s.is_empty())
}

fn canonical_instruments(part: &Element) -> Vec<String> {
    children(part, "Instrument")
        .map(|instrument| {
            child_text(instrument, "instrumentId")
                .filter(|id| !id.is_empty())
                .or_else(|| instrument_id(instrument).map(str::to_owned))
                .unwrap_or_default()
        })
        .collect()
}

fn flatten(node: &mut Element) {
    match node.name.as_str() {
        "Note" => {
            node.children
                .retain(|n| n.as_element().is_none_or(|e| e.name != "veloType"));
            set_child(node, "velocity", "80");
        }
        "Dynamic" => {
            set_child(node, "play", "0");
            set_child(node, "velocity", "80");
            set_child(node, "veloChange", "0");
        }
        "HairPin" => {
            set_child(node, "play", "0");
            set_child(node, "veloChange", "0");
            set_child(node, "singleNoteDynamics", "0");
        }
        "Instrument" => {
            set_child(node, "singleNoteDynamics", "0");
            for art in node
                .children
                .iter_mut()
                .filter_map(XMLNode::as_mut_element)
                .filter(|e| e.name == "Articulation")
            {
                set_child(art, "velocity", "100");
            }
        }
        "SoundFlag" | "Harmony" => {
            set_child(node, "play", "0");
        }
        "StaffText" | "SystemText" => node.children.retain(|n| {
            !n.as_element()
                .is_some_and(|e| matches!(e.name.as_str(), "MidiAction" | "channelSwitch"))
        }),
        _ => (),
    }
}

#[allow(clippy::too_many_arguments)]
fn configure(
    instrument: &mut Element,
    part_id: &str,
    key: &str,
    target: &str,
    options: &PracticeOptions,
    catalog: &SoundCatalog,
    tracks: &mut Vec<Value>,
    known: &mut HashSet<(String, String)>,
) -> Result<()> {
    let setting = options
        .parts
        .iter()
        .find(|p| p.key == key)
        .ok_or("Unknown part")?;
    let foreground = key == target;
    let active = foreground || setting.include_in_background;
    let id = instrument_id(instrument)
        .map(str::to_owned)
        .or_else(|| child_text(instrument, "instrumentId"))
        .filter(|s| !s.is_empty())
        .ok_or("Normalized instrument has no ID")?;
    // Inactive tracks keep a valid available preset but are muted in both playback layers.
    let preset = if active {
        catalog.get(
            if foreground {
                setting.foreground_sound
            } else {
                setting.background_sound
            }
            .ok_or("Missing instrument sound")?,
        )?
    } else {
        catalog.presets.first().ok_or("MS Basic has no presets")?
    };
    if !elements(instrument).any(|e| e.name == "Channel") {
        instrument
            .children
            .push(XMLNode::Element(Element::new("Channel")));
    }
    for channel in instrument
        .children
        .iter_mut()
        .filter_map(XMLNode::as_mut_element)
        .filter(|e| e.name == "Channel")
    {
        channel.children.retain(|n| {
            !n.as_element()
                .is_some_and(|e| matches!(e.name.as_str(), "program" | "controller" | "MidiAction"))
        });
        let mut program = Element::new("program");
        program
            .attributes
            .insert("value".into(), preset.program.to_string());
        channel.children.push(XMLNode::Element(program));
        for (ctrl, value) in [
            (0, preset.bank >> 7),
            (32, preset.bank & 127),
            (7, 100),
            (11, 127),
            (10, 64),
        ] {
            let mut controller = Element::new("controller");
            controller
                .attributes
                .insert("ctrl".into(), ctrl.to_string());
            controller
                .attributes
                .insert("value".into(), value.to_string());
            channel.children.push(XMLNode::Element(controller));
        }
        set_child(channel, "synti", "Fluid");
        set_child(channel, "mute", if active { "0" } else { "1" });
        set_child(channel, "solo", "0");
    }
    if known.insert((part_id.into(), id.clone())) {
        tracks.push(json!({"partId":part_id,"instrumentId":id,
            "in":{"resourceMeta":resource(preset),"unitConfiguration":{}},
            "out":output(if foreground { 0.0 } else { -options.background_reduction_db }),
            "soloMuteState":{"mute":!active,"solo":false}}));
    }
    Ok(())
}

fn output(db: f64) -> Value {
    json!({"balance":0,"volumeDb":db,"fxChain":{},"auxSends":[{"active":false,"signalAmount":0},{"active":false,"signalAmount":0}]})
}

fn resource(preset: &Preset) -> Value {
    json!({"id":format!("MS Basic\\{}\\{}",preset.bank,preset.program),"type":"fluid_soundfont","vendor":"Fluid","hasNativeEditorSupport":false,
        "attributes":{"soundFontName":"MS Basic","presetName":preset.name,"presetBank":preset.bank.to_string(),"presetProgram":preset.program.to_string()}})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PartSettings;

    #[test]
    fn conversion_filters_explicit_visibility_and_remaps_staffs_without_changing_source() {
        let original = source();
        let converted = original.visible_score().unwrap();
        assert_eq!(converted.parts().unwrap().len(), 2);
        assert_eq!(
            children(converted.master(), "Staff")
                .map(|staff| staff.attributes["id"].as_str())
                .collect::<Vec<_>>(),
            ["1", "2", "3"]
        );
        assert_eq!(original.parts().unwrap().len(), 3);
        let mut single = source();
        let part = single.master_mut().get_mut_child("Part").unwrap();
        set_child(part.get_mut_child("Staff").unwrap(), "show", "0");
        let single = single.visible_score().unwrap();
        assert_eq!(
            children(single.master().get_child("Part").unwrap(), "Staff").count(),
            1
        );
        assert!(!format!("{:?}", single.master()).contains("pitch"));
    }

    #[test]
    fn conversion_rejects_empty_and_cross_staff_scores() {
        let mut empty = source();
        for part in empty
            .master_mut()
            .children
            .iter_mut()
            .filter_map(XMLNode::as_mut_element)
            .filter(|node| node.name == "Part")
        {
            set_child(part, "show", "0");
        }
        assert!(empty.visible_score().unwrap_err().contains("no visible"));
        let mut linked = source();
        set_child(
            linked.master_mut().get_mut_child("Staff").unwrap(),
            "staffMove",
            "1",
        );
        assert!(linked.visible_score().unwrap_err().contains("cross-staff"));
        let mut references = source();
        references
            .master_mut()
            .get_mut_child("Part")
            .unwrap()
            .get_mut_child("Instrument")
            .unwrap()
            .attributes
            .insert("staff".into(), "2".into());
        assert!(references
            .visible_score()
            .unwrap_err()
            .contains("attribute references"));
    }

    #[test]
    fn conversion_supports_compressed_archives_and_ordinal_staff_ids() {
        let original = source();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("score.mscz");
        original.write(&path).unwrap();
        let mut restored =
            ScoreArchive::read_bytes(&std::fs::read(path).unwrap(), "score.mscz").unwrap();
        for part in restored
            .master_mut()
            .children
            .iter_mut()
            .filter_map(XMLNode::as_mut_element)
            .filter(|node| node.name == "Part")
        {
            for staff in part
                .children
                .iter_mut()
                .filter_map(XMLNode::as_mut_element)
                .filter(|node| node.name == "Staff")
            {
                staff.attributes.remove("id");
            }
        }
        assert_eq!(
            children(restored.visible_score().unwrap().master(), "Staff").count(),
            3
        );
    }

    fn source() -> ScoreArchive {
        ScoreArchive::read_bytes(br#"<museScore version="4.70"><Score><Division>480</Division>
          <Part id="1"><Staff id="1"/><Staff id="2"/><trackName>Same</trackName><Instrument id="piano"><Channel><program value="0"/><synti>Fluid</synti></Channel><Articulation><velocity>150</velocity><gateTime>67</gateTime></Articulation></Instrument></Part>
          <Part id="2"><Staff id="3"/><show>0</show><trackName>Same</trackName><Instrument id="violin"><Channel><program value="40"/></Channel></Instrument></Part>
          <Part id="3"><Staff id="4"/><trackName>Other</trackName><Instrument id="piano"><Channel><program value="0"/></Channel></Instrument></Part>
          <Staff id="1"><Measure><voice><Tempo><tempo>2</tempo></Tempo><Dynamic><velocity>20</velocity></Dynamic><HairPin><veloChange>60</veloChange></HairPin><Harmony><root>14</root><name>7</name><play>1</play></Harmony><Chord><durationType>quarter</durationType><Note><pitch>60</pitch><veloType>offset</veloType><velocity>15</velocity></Note></Chord><SoundFlag><play>1</play></SoundFlag><StaffText><channelSwitch>2</channelSwitch><MidiAction/></StaffText><InstrumentChange><Instrument id="oboe"><Channel/></Instrument></InstrumentChange></voice></Measure></Staff>
          <Staff id="2"/><Staff id="3"/><Staff id="4"/>
        </Score></museScore>"#, "test").unwrap()
    }

    fn catalog() -> SoundCatalog {
        SoundCatalog {
            presets: vec![
                Preset {
                    bank: 0,
                    program: 0,
                    name: "Piano".into(),
                },
                Preset {
                    bank: 0,
                    program: 40,
                    name: "Violin".into(),
                },
            ],
        }
    }

    fn options() -> PracticeOptions {
        PracticeOptions {
            parts: (1..=3)
                .map(|index| PartSettings {
                    key: format!("part-{index}"),
                    foreground_sound: Some(PresetId {
                        bank: 0,
                        program: 40,
                    }),
                    background_sound: Some(PresetId {
                        bank: 0,
                        program: 0,
                    }),
                    include_in_background: index == 1,
                    create_track: index != 3,
                })
                .collect(),
            background_reduction_db: 9.0,
            metronome_enabled: true,
            metronome_db: -12.0,
        }
    }

    fn native(score: &mut ScoreArchive) {
        score.audio = json!({"tracks":[{"partId":"999","instrumentId":"metronome","in":{"resourceMeta":{"id":"native-click"}}},{"partId":"88","instrumentId":"harmony"}]});
    }

    fn zip_entries(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn inspect_identity_visibility_and_sound() {
        let score = source();
        let parts = score.parts().unwrap();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].name, parts[1].name);
        assert_ne!(parts[0].key, parts[1].key);
        assert!(!parts[1].visible);
        assert_eq!(
            parts[0].existing_preset,
            Some(PresetId {
                bank: 0,
                program: 0
            })
        );
        assert_eq!(plain_text("<b>Violin</b> &amp; flute"), "Violin & flute");
        score.verify_normalized(&score).unwrap();
        let mut normalized = score.clone();
        normalized
            .master_mut()
            .get_mut_child("Part")
            .unwrap()
            .attributes
            .remove("id");
        assert!(normalized.verify_normalized(&score).is_err());
        let mut normalized = score.clone();
        normalized
            .master_mut()
            .get_mut_child("Part")
            .unwrap()
            .attributes
            .insert("id".into(), "2".into());
        assert!(normalized.verify_normalized(&score).is_err());
    }

    #[test]
    fn normalized_implicit_staff_order_and_canonical_instrument_identity() {
        let mut original = source();
        original
            .master_mut()
            .get_mut_child("Part")
            .unwrap()
            .get_mut_child("Instrument")
            .unwrap()
            .attributes
            .remove("id");
        set_child(
            original
                .master_mut()
                .get_mut_child("Part")
                .unwrap()
                .get_mut_child("Instrument")
                .unwrap(),
            "instrumentId",
            "keyboard.piano",
        );
        let mut normalized = original.clone();
        normalized
            .master_mut()
            .get_mut_child("Part")
            .unwrap()
            .get_mut_child("Instrument")
            .unwrap()
            .attributes
            .insert("id".into(), "piano".into());
        for part in normalized
            .master_mut()
            .children
            .iter_mut()
            .filter_map(XMLNode::as_mut_element)
            .filter(|node| node.name == "Part")
        {
            for staff in part
                .children
                .iter_mut()
                .filter_map(XMLNode::as_mut_element)
                .filter(|node| node.name == "Staff")
            {
                staff.attributes.remove("id");
            }
        }
        normalized.verify_normalized(&original).unwrap();
        let mix = normalized.mix(&options(), "part-1", &catalog()).unwrap();
        assert_eq!(
            mix.audio["tracks"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|track| track["partId"] == "1")
                .count(),
            2
        );
        assert_eq!(children(mix.master(), "Part").count(), 3);
    }

    #[test]
    fn independent_mixes_override_changes_and_preserve_notation() {
        let mut score = source();
        native(&mut score);
        let original = score.root.clone();
        let first = score.mix(&options(), "part-1", &catalog()).unwrap();
        let second = score.mix(&options(), "part-2", &catalog()).unwrap();
        assert_eq!(score.root, original);
        let tracks = second.audio["tracks"].as_array().unwrap();
        assert_eq!(tracks.iter().filter(|t| t["partId"] == "1").count(), 2);
        for t in tracks {
            let id = t["partId"].as_str().unwrap();
            assert_eq!(t["soloMuteState"]["mute"], matches!(id, "3" | "88"));
            assert_eq!(t["soloMuteState"]["solo"], false);
            if id == "1" {
                assert_eq!(t["out"]["volumeDb"], -9.0);
                assert_eq!(t["in"]["resourceMeta"]["attributes"]["presetProgram"], "0");
            }
            if id == "2" {
                assert_eq!(t["out"]["volumeDb"], 0.0);
                assert_eq!(t["in"]["resourceMeta"]["attributes"]["presetProgram"], "40");
            }
            if id == "999" {
                assert_eq!(t["out"]["volumeDb"], -12.0);
                assert_eq!(t["in"]["resourceMeta"]["id"], "native-click");
            }
        }
        assert_eq!(
            first.audio["tracks"][0]["in"]["resourceMeta"]["attributes"]["presetProgram"],
            "40"
        );
        assert_eq!(
            children(second.master(), "Part")
                .nth(1)
                .unwrap()
                .get_child("show")
                .unwrap()
                .get_text()
                .unwrap(),
            "1"
        );
        assert_eq!(children(second.master(), "Part").count(), 3); // Native click never creates a score part.
        visit(second.master(), &mut |node| match node.name.as_str() {
            "Note" => {
                assert_eq!(child_text(node, "velocity").unwrap(), "80");
                assert!(node.get_child("veloType").is_none());
                assert_eq!(child_text(node, "pitch").unwrap(), "60");
            }
            "HairPin" | "Dynamic" | "SoundFlag" | "Harmony" => {
                assert_eq!(child_text(node, "play").unwrap(), "0")
            }
            "Articulation" => {
                assert_eq!(child_text(node, "velocity").unwrap(), "100");
                assert_eq!(child_text(node, "gateTime").unwrap(), "67");
            }
            "Tempo" => assert_eq!(child_text(node, "tempo").unwrap(), "2"),
            "StaffText" => assert_eq!(elements(node).count(), 0),
            _ => (),
        });
    }

    #[test]
    fn validates_role_membership_and_native_track() {
        let mut score = source();
        let mut request = options();
        let mixed = score.mix(&request, "part-1", &catalog()).unwrap();
        assert_eq!(children(mixed.master(), "Part").count(), 3);
        assert_eq!(
            mixed.audio["tracks"].as_array().unwrap().last().unwrap()["instrumentId"],
            "metronome"
        );
        native(&mut score);
        request.metronome_enabled = false;
        request
            .parts
            .iter_mut()
            .for_each(|p| p.include_in_background = false);
        let mix = score.mix(&request, "part-2", &catalog()).unwrap();
        for t in mix.audio["tracks"].as_array().unwrap() {
            assert_eq!(t["soloMuteState"]["mute"], t["partId"] != "2");
        }
        request.parts[0].foreground_sound = None;
        assert!(request
            .validate(&score.parts().unwrap(), &catalog())
            .is_err());
        request = options();
        request.parts[0].key = request.parts[1].key.clone();
        assert!(request
            .validate(&score.parts().unwrap(), &catalog())
            .is_err());
        request = options();
        request
            .parts
            .iter_mut()
            .for_each(|p| p.create_track = false);
        assert!(request
            .validate(&score.parts().unwrap(), &catalog())
            .is_err());
        request = options();
        request.background_reduction_db = f64::NAN;
        assert!(request
            .validate(&score.parts().unwrap(), &catalog())
            .is_err());
    }

    #[test]
    fn excluded_notes_are_unplayed_without_changing_notation_or_active_note_flags() {
        let mut source = source();
        let hidden = source
            .master_mut()
            .children
            .iter_mut()
            .filter_map(XMLNode::as_mut_element)
            .find(|e| e.name == "Staff" && e.attributes.get("id").is_some_and(|id| id == "4"))
            .unwrap();
        hidden.children = parse_xml(br#"<Staff><Measure><voice><Chord><durationType>quarter</durationType><Note><pitch>80</pitch><tpc>16</tpc></Note></Chord></voice></Measure></Staff>"#, "test").unwrap().children;
        let mix = source.mix(&options(), "part-1", &catalog()).unwrap();
        let hidden = children(mix.master(), "Staff")
            .find(|staff| staff.attributes["id"] == "4")
            .unwrap();
        visit(hidden, &mut |node| {
            if node.name == "Note" {
                assert_eq!(child_text(node, "play").as_deref(), Some("0"));
                assert_eq!(child_text(node, "pitch").as_deref(), Some("80"));
                assert_eq!(child_text(node, "tpc").as_deref(), Some("16"));
            }
        });
        let active = children(mix.master(), "Staff")
            .find(|staff| staff.attributes["id"] == "1")
            .unwrap();
        visit(active, &mut |node| {
            if node.name == "Note" {
                assert!(node.get_child("play").is_none());
            }
        });
    }

    #[test]
    fn round_trip_preserves_assets_and_json_is_checked() {
        let mut score = source();
        native(&mut score);
        score
            .entries
            .insert("assets/keep.bin".into(), Arc::from(&b"unchanged"[..]));
        let file = tempfile::NamedTempFile::new().unwrap();
        score.write(file.path()).unwrap();
        let bytes = std::fs::read(file.path()).unwrap();
        let copy = ScoreArchive::read_bytes(&bytes, "roundtrip").unwrap();
        assert_eq!(&*copy.entries["assets/keep.bin"], b"unchanged");
        assert_eq!(copy.parts().unwrap().len(), 3);
        assert_eq!(copy.audio, score.audio);
        let mut normalized = source();
        normalized.merge_assets_from(&copy);
        assert!(Arc::ptr_eq(
            &normalized.entries["assets/keep.bin"],
            &copy.entries["assets/keep.bin"]
        ));
        for value in [
            json!([]),
            json!({"tracks":{}}),
            json!({"tracks":[null]}),
            json!({"tracks":[{"partId":"1","instrumentId":"x","in":{"resourceMeta":[]}}]}),
        ] {
            assert!(validate_audio(&value).is_err());
        }
        for name in [
            "../score.mscx",
            "/score.mscx",
            "C:/score.mscx",
            "a\\b",
            "a/./b",
        ] {
            assert!(safe_name(name).is_err());
        }
        let invalid = zip_entries(&[("../bad", b"bad")]);
        assert!(ScoreArchive::read_bytes(&invalid, "bad")
            .unwrap_err()
            .contains("Unsafe"));
        assert!(
            ScoreArchive::read_bytes(b"<museScore version='3.0'><Score/></museScore>", "old")
                .is_err()
        );
        assert!(ScoreArchive::read_bytes(&vec![0; MEMBER_LIMIT + 1], "large").is_err());
    }

    #[test]
    fn zip_rejects_duplicate_and_oversized_members_and_invalid_audio() {
        let mut duplicate = zip_entries(&[("one", b"x"), ("two", b"y")]);
        for index in 0..duplicate.len() - 2 {
            if &duplicate[index..index + 3] == b"two" {
                duplicate[index..index + 3].copy_from_slice(b"one");
            }
        }
        let error = ScoreArchive::read_bytes(&duplicate, "duplicate").unwrap_err();
        assert!(error.contains("Duplicate"), "{error}");
        let mut oversized = zip_entries(&[("large", b"x")]);
        for index in 0..oversized.len() - 46 {
            if &oversized[index..index + 4] == b"PK\x01\x02" {
                oversized[index + 24..index + 28]
                    .copy_from_slice(&((MEMBER_LIMIT + 1) as u32).to_le_bytes());
            }
        }
        assert!(ScoreArchive::read_bytes(&oversized, "large")
            .unwrap_err()
            .contains("64 MiB"));
        let mut xml = Vec::new();
        source().root.write(&mut xml).unwrap();
        let metadata =
            br#"<container><rootfiles><rootfile full-path="score.mscx"/></rootfiles></container>"#;
        for audio in [
            b"{not json".as_slice(),
            b"{\"tracks\":[{\"partId\":1}]}".as_slice(),
        ] {
            let bytes = zip_entries(&[
                ("META-INF/container.xml", metadata),
                ("score.mscx", &xml),
                ("audiosettings.json", audio),
            ]);
            assert!(
                ScoreArchive::read_bytes(&bytes, "invalid")
                    .unwrap_err()
                    .contains("audio")
                    || ScoreArchive::read_bytes(&bytes, "invalid")
                        .unwrap_err()
                        .contains("Audio")
            );
        }
    }
}
