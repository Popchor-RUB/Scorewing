use super::*;
use crate::PracticeSection;
use std::ops::RangeInclusive;

impl ScoreArchive {
    pub fn rehearsal_sections(&self) -> Result<Vec<PracticeSection>> {
        let mut marks: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        let mut measure_count = None;
        let mut consistent = true;
        for staff in children(self.master(), "Staff") {
            let measures: Vec<_> = written_measures(staff).collect();
            if measure_count.is_some_and(|count| count != measures.len()) {
                consistent = false;
            }
            measure_count = Some(measures.len());
            for (index, measure) in measures.iter().enumerate() {
                visit(measure, &mut |node| {
                    if node.name != "RehearsalMark" {
                        return;
                    }
                    let text = node.get_child("text").map(element_text).unwrap_or_default();
                    let name = plain_text(&text).trim().to_owned();
                    let name = if name.is_empty() {
                        format!("Rehearsal at measure {}", index + 1)
                    } else {
                        name
                    };
                    let names = marks.entry(index + 1).or_default();
                    // System marks may be repeated on more than one staff in the XML.
                    if !names.contains(&name) {
                        names.push(name);
                    }
                });
            }
        }
        let Some((&first, _)) = marks.first_key_value() else {
            return Ok(vec![]);
        };
        if !consistent {
            return Err("Score staves have inconsistent measure counts".into());
        }
        if first > 1 {
            marks.insert(1, vec!["Intro".into()]);
        }
        let starts: Vec<_> = marks.keys().copied().collect();
        Ok(marks
            .into_iter()
            .enumerate()
            .map(|(index, (start, names))| PracticeSection {
                key: format!("measure-{start}"),
                name: names.join(" + "),
                start_measure: start,
                end_measure: starts
                    .get(index + 1)
                    .map_or(measure_count.unwrap(), |n| n - 1),
            })
            .collect())
    }

    pub(crate) fn selected_sections(
        &self,
        options: &PracticeOptions,
    ) -> Result<Vec<PracticeSection>> {
        let sections = self.rehearsal_sections()?;
        let mut seen = HashSet::new();
        for key in &options.section_keys {
            if !seen.insert(key) || !sections.iter().any(|section| &section.key == key) {
                return Err("Unknown or duplicate rehearsal section".into());
            }
        }
        if !options.include_whole_song && seen.is_empty() {
            return Err("Select the whole song or at least one rehearsal section".into());
        }
        Ok(sections
            .into_iter()
            .filter(|s| seen.contains(&s.key))
            .collect())
    }

    pub(crate) fn playback_source(&self) -> Self {
        let mut score = self.clone();
        let master = score.master_mut();
        if master.get_child("Style").is_none() {
            master
                .children
                .insert(0, XMLNode::Element(Element::new("Style")));
        }
        set_child(
            master.get_mut_child("Style").unwrap(),
            "createMultiMeasureRests",
            "0",
        );
        for staff in master
            .children
            .iter_mut()
            .filter_map(XMLNode::as_mut_element)
            .filter(|n| n.name == "Staff")
        {
            // Cached layout bars otherwise collapse .mpos IDs and are not written measures.
            staff.children.retain(|child| {
                child.as_element().is_none_or(|n| {
                    n.name != "Measure" || n.get_child("multiMeasureRest").is_none()
                })
            });
        }
        score
    }

    pub(crate) fn prepare_playback(
        &self,
        expanded: &mut Self,
        positions: &[u8],
        sections: &[PracticeSection],
    ) -> Result<Vec<RangeInclusive<usize>>> {
        expanded.verify_normalized(self)?;
        let written = consistent_measure_count(self)?;
        let played = consistent_measure_count(expanded)?;
        let map = parse_xml(positions, "playback.mpos")
            .map_err(|e| format!("Invalid MuseScore playback positions: {e}"))?;
        let invalid = || "MuseScore playback positions do not match the expanded score".to_owned();
        if map.name != "score" {
            return Err(invalid());
        }
        let elements = map.get_child("elements").ok_or_else(invalid)?;
        let ids: Vec<_> = children(elements, "element")
            .map(|node| {
                node.attributes
                    .get("id")
                    .and_then(|id| id.parse::<usize>().ok())
            })
            .collect();
        if ids.len() != written || ids.iter().enumerate().any(|(index, id)| *id != Some(index)) {
            return Err(invalid());
        }
        let events = map.get_child("events").ok_or_else(invalid)?;
        let mut route = vec![];
        let mut previous = 0;
        for event in children(events, "event") {
            let id = event
                .attributes
                .get("elid")
                .and_then(|id| id.parse::<usize>().ok())
                .ok_or_else(invalid)?;
            let position = event
                .attributes
                .get("position")
                .and_then(|p| p.parse::<u64>().ok())
                .ok_or_else(invalid)?;
            if id >= written || position < previous || (route.is_empty() && position != 0) {
                return Err(invalid());
            }
            previous = position;
            route.push(id + 1);
        }
        if route.len() != played {
            return Err(invalid());
        }
        let ranges = sections
            .iter()
            .map(|section| section_playback_range(&route, section))
            .collect::<Result<Vec<_>>>()?;
        let tempos = inherited_tempos(self);
        let staff = expanded.master_mut().get_mut_child("Staff").unwrap();
        for (index, measure) in staff
            .children
            .iter_mut()
            .filter_map(XMLNode::as_mut_element)
            .filter(|n| n.name == "Measure")
            .enumerate()
            .skip(1)
        {
            if route[index] != route[index - 1] + 1 {
                // Native unrolling restores meter/key at jumps, but drops inherited tempo.
                let tempo = tempos[route[index] - 1]
                    .as_deref()
                    .cloned()
                    .unwrap_or_else(|| {
                        let mut tempo = Element::new("Tempo");
                        set_child(&mut tempo, "tempo", "2");
                        tempo
                    });
                insert_initial_tempo(measure, tempo);
            }
        }
        Ok(ranges)
    }

    pub(crate) fn region(&self, range: &RangeInclusive<usize>) -> Value {
        json!({
            "start": {"measure": range.start(), "staff": 1},
            "end": {"measure": range.end(), "staff": children(self.master(), "Staff").count()}
        })
    }

    pub(crate) fn without_repeats(&self) -> Self {
        let mut score = self.clone();
        visit_mut(score.master_mut(), &mut |node| {
            node.children.retain(|child| {
                child.as_element().is_none_or(|e| {
                    !(matches!(
                        e.name.as_str(),
                        "startRepeat" | "endRepeat" | "Jump" | "Marker"
                    ) || (e.name == "Spanner"
                        && e.attributes.get("type").is_some_and(|t| t == "Volta")))
                })
            });
        });
        score
    }

    pub(crate) fn verify_region(&self, source: &Self, range: &RangeInclusive<usize>) -> Result<()> {
        self.verify_normalized(source)?;
        let expected = range.end() - range.start() + 1;
        let staves: Vec<_> = children(self.master(), "Staff").collect();
        if staves.is_empty()
            || staves.len() != children(source.master(), "Staff").count()
            || staves
                .iter()
                .any(|staff| children(staff, "Measure").count() != expected)
        {
            return Err("MuseScore exported an unexpected section length".into());
        }
        Ok(())
    }

    pub(crate) fn prepare_section(&mut self, source: &Self, range: &RangeInclusive<usize>) {
        for staff in self
            .master_mut()
            .children
            .iter_mut()
            .filter_map(XMLNode::as_mut_element)
            .filter(|node| node.name == "Staff")
        {
            let count = children(staff, "Measure").count() as i64;
            for (index, measure) in staff
                .children
                .iter_mut()
                .filter_map(XMLNode::as_mut_element)
                .filter(|node| node.name == "Measure")
                .enumerate()
            {
                visit_mut(measure, &mut |node| {
                    // Native selection keeps ties to notes outside the exported score.
                    node.children.retain(|child| {
                        child.as_element().is_none_or(|spanner| {
                            !tie_outside_section(spanner, index as i64, count)
                        })
                    });
                });
            }
        }
        if let Some(tempo) = inherited_tempos(source)[range.start() - 1].as_deref() {
            // Native region export drops tempo annotations before the selected measures.
            let measure = self
                .master_mut()
                .get_mut_child("Staff")
                .unwrap()
                .get_mut_child("Measure")
                .unwrap();
            insert_initial_tempo(measure, tempo.clone());
        }
    }
}

fn inherited_tempos(score: &ScoreArchive) -> Vec<Option<Arc<Element>>> {
    let count = children(score.master(), "Staff")
        .next()
        .map_or(0, |staff| children(staff, "Measure").count());
    let mut changes = vec![None; count];
    for staff in children(score.master(), "Staff") {
        for (index, measure) in children(staff, "Measure").enumerate() {
            visit(measure, &mut |node| {
                if node.name == "Tempo" {
                    let mut tempo = node.clone();
                    tempo
                        .children
                        .retain(|child| child.as_element().is_none_or(|e| e.name != "eid"));
                    // Restore the normalized rate; do not reinterpret relative tempo text.
                    set_child(&mut tempo, "followText", "0");
                    changes[index] = Some(Arc::new(tempo));
                }
            });
        }
    }
    let mut active = None;
    changes
        .into_iter()
        .map(|change| {
            let inherited = active.clone();
            if change.is_some() {
                active = change;
            }
            inherited
        })
        .collect()
}

fn insert_initial_tempo(measure: &mut Element, tempo: Element) {
    if measure.get_child("voice").is_none() {
        measure
            .children
            .push(XMLNode::Element(Element::new("voice")));
    }
    measure
        .get_mut_child("voice")
        .unwrap()
        .children
        .insert(0, XMLNode::Element(tempo));
}

fn written_measures(staff: &Element) -> impl Iterator<Item = &Element> {
    children(staff, "Measure").filter(|measure| measure.get_child("multiMeasureRest").is_none())
}

fn consistent_measure_count(score: &ScoreArchive) -> Result<usize> {
    let counts: Vec<_> = children(score.master(), "Staff")
        .map(|staff| {
            let count = written_measures(staff).count();
            (count, children(staff, "Measure").count())
        })
        .collect();
    match counts.first() {
        Some(&(count, _)) if count > 0 && counts.iter().all(|n| *n == (count, count)) => Ok(count),
        _ => Err("MuseScore playback score has empty or inconsistent measure counts".into()),
    }
}

fn section_playback_range(
    route: &[usize],
    section: &PracticeSection,
) -> Result<RangeInclusive<usize>> {
    let start = route
        .iter()
        .position(|n| *n == section.start_measure)
        .ok_or_else(|| {
            format!(
                "Rehearsal section {} is unreachable in score playback",
                section.name
            )
        })?;
    let end = route[start..]
        .iter()
        .position(|n| !(section.start_measure..=section.end_measure).contains(n))
        .map_or(route.len(), |exit| (start + exit + 2).min(route.len()));
    Ok(start + 1..=end)
}

fn tie_outside_section(spanner: &Element, measure: i64, count: i64) -> bool {
    if spanner.name != "Spanner" || spanner.attributes.get("type").is_none_or(|t| t != "Tie") {
        return false;
    }
    ["prev", "next"].iter().any(|direction| {
        spanner
            .get_child(*direction)
            .and_then(|n| n.get_child("location"))
            .is_some_and(|location| {
                let offset = child_text(location, "measures")
                    .and_then(|text| text.parse::<i64>().ok())
                    .unwrap_or(0);
                offset
                    .checked_add(measure)
                    .is_none_or(|target| target < 0 || target >= count)
            })
    })
}

fn element_text(node: &Element) -> String {
    node.children
        .iter()
        .map(|child| match child {
            XMLNode::Text(text) | XMLNode::CData(text) => text.clone(),
            XMLNode::Element(element) => element_text(element),
            _ => String::new(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score(measures: &str) -> ScoreArchive {
        ScoreArchive::read_bytes(format!(r#"<museScore version="4.70"><Score>
          <Part id="1"><Staff id="1"/><trackName>Piano</trackName><Instrument id="piano"><Channel/></Instrument></Part>
          <Staff id="1">{measures}</Staff></Score></museScore>"#).as_bytes(), "sections").unwrap()
    }

    #[test]
    fn discovers_measure_ranges_and_formatted_duplicate_and_same_bar_marks() {
        let source = score(
            r#"
          <Measure><voice><StaffText><text>Boxed text</text><frameType>1</frameType></StaffText></voice></Measure>
          <Measure><voice><Chord/><RehearsalMark><text>&lt;b&gt;A&lt;/b&gt;</text></RehearsalMark><RehearsalMark><text>Trio</text></RehearsalMark></voice></Measure>
          <Measure/><Measure><voice><RehearsalMark><text><b>A</b></text></RehearsalMark></voice></Measure>
          <Measure><voice><RehearsalMark><text>../B:*</text></RehearsalMark></voice></Measure>"#,
        );
        let sections = source.rehearsal_sections().unwrap();
        assert_eq!(
            sections
                .iter()
                .map(|s| (
                    s.key.as_str(),
                    s.name.as_str(),
                    s.start_measure,
                    s.end_measure
                ))
                .collect::<Vec<_>>(),
            [
                ("measure-1", "Intro", 1, 1),
                ("measure-2", "A + Trio", 2, 3),
                ("measure-4", "A", 4, 4),
                ("measure-5", "../B:*", 5, 5),
            ]
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("score.mscz");
        source.write(&path).unwrap();
        assert_eq!(
            ScoreArchive::read_bytes(&std::fs::read(path).unwrap(), "compressed")
                .unwrap()
                .rehearsal_sections()
                .unwrap(),
            sections
        );
    }

    #[test]
    fn no_marks_first_measure_marks_and_blank_labels() {
        assert!(score("<Measure/>").rehearsal_sections().unwrap().is_empty());
        let sections =
            score("<Measure><voice><RehearsalMark><text/></RehearsalMark></voice></Measure>")
                .rehearsal_sections()
                .unwrap();
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].name, "Rehearsal at measure 1");
        assert_eq!((sections[0].start_measure, sections[0].end_measure), (1, 1));
    }

    #[test]
    fn section_requests_are_validated_and_sorted_by_score_order() {
        let source = score("<Measure/><Measure><voice><RehearsalMark><text>A</text></RehearsalMark></voice></Measure>");
        let mut options: PracticeOptions = serde_json::from_value(json!({
            "parts": [], "background_reduction_db": 6, "metronome_enabled": false, "metronome_db": -6
        })).unwrap();
        assert!(
            options.include_whole_song,
            "old requests keep whole-song behavior"
        );
        assert!(source.selected_sections(&options).unwrap().is_empty());
        options.include_whole_song = false;
        assert!(source
            .selected_sections(&options)
            .unwrap_err()
            .contains("at least one"));
        options.section_keys = vec!["measure-2".into(), "measure-1".into()];
        assert_eq!(source.selected_sections(&options).unwrap()[0].name, "Intro");
        options.section_keys.push("measure-2".into());
        assert!(source.selected_sections(&options).is_err());
        options.section_keys = vec!["../../escape".into()];
        assert!(source.selected_sections(&options).is_err());
    }

    #[test]
    fn section_copies_remove_navigation_and_restore_inherited_tempo() {
        let source = score(
            r#"<Measure><startRepeat/><voice><Tempo><tempo>1</tempo><text>60</text></Tempo><Jump/><Marker/></voice></Measure>
          <Measure><endRepeat>2</endRepeat><voice><RehearsalMark><text>A</text></RehearsalMark><Spanner type="Volta"/><Chord/></voice></Measure>"#,
        );
        let section = &source.rehearsal_sections().unwrap()[1];
        let straight = source.without_repeats();
        visit(straight.master(), &mut |node| {
            assert!(!matches!(
                node.name.as_str(),
                "startRepeat" | "endRepeat" | "Jump" | "Marker" | "Spanner"
            ))
        });
        assert!(
            children(source.master().get_child("Staff").unwrap(), "Measure")
                .next()
                .unwrap()
                .get_child("startRepeat")
                .is_some()
        );
        let mut region =
            score("<Measure><voice><Chord/><Tempo><tempo>3</tempo></Tempo></voice></Measure>");
        let range = section.start_measure..=section.end_measure;
        region.verify_region(&source, &range).unwrap();
        region.prepare_section(&source, &range);
        let voice = region
            .master()
            .get_child("Staff")
            .unwrap()
            .get_child("Measure")
            .unwrap()
            .get_child("voice")
            .unwrap();
        assert_eq!(elements(voice).next().unwrap().name, "Tempo");
        assert_eq!(
            child_text(elements(voice).next().unwrap(), "tempo").as_deref(),
            Some("1")
        );
        assert_eq!(
            children(voice, "Tempo").count(),
            2,
            "later changes remain intact"
        );
    }

    #[test]
    fn section_preparation_removes_only_ties_crossing_its_boundaries() {
        let source = score("<Measure/><Measure><voice><RehearsalMark><text>A</text></RehearsalMark></voice></Measure><Measure/>");
        let section = &source.rehearsal_sections().unwrap()[1];
        let mut region = score(
            r#"<Measure><voice><Chord><Note>
          <Spanner type="Tie"><prev><location><measures>-1</measures></location></prev></Spanner>
          <Spanner type="Tie"><Tie/><next><location><measures>1</measures></location></next></Spanner>
          <Spanner type="Slur"/></Note></Chord></voice></Measure>
          <Measure><voice><Chord><Note>
          <Spanner type="Tie"><prev><location><measures>-1</measures></location></prev></Spanner>
          <Spanner type="Tie"><Tie/><next><location><measures>1</measures></location></next></Spanner>
          </Note></Chord></voice></Measure>"#,
        );
        let range = section.start_measure..=section.end_measure;
        region.verify_region(&source, &range).unwrap();
        region.prepare_section(&source, &range);
        let mut types = vec![];
        visit(region.master(), &mut |node| {
            if node.name == "Spanner" {
                types.push(node.attributes["type"].clone());
            }
        });
        assert_eq!(types, ["Tie", "Slur", "Tie"]);
    }

    #[test]
    fn section_ranges_follow_first_arrival_internal_repeats_and_two_played_tail_bars() {
        let section = PracticeSection {
            key: "measure-3".into(),
            name: "B".into(),
            start_measure: 3,
            end_measure: 4,
        };
        for (route, expected) in [
            (vec![1, 2, 3, 4, 5, 6, 7], 3..=6),
            (vec![1, 2, 3, 4, 1, 2, 3, 4], 3..=6),
            (vec![1, 2, 3, 4, 3, 4, 5, 6, 7], 3..=8),
            // Navigation during the tail can return into B; it still counts as the second bar.
            (vec![1, 2, 3, 4, 1, 3, 4, 5], 3..=6),
            (vec![1, 2, 3, 4, 5], 3..=5),
            (vec![1, 2, 3], 3..=3),
            (vec![1, 2, 3, 4, 3, 4], 3..=6),
        ] {
            assert_eq!(
                section_playback_range(&route, &section).unwrap(),
                expected,
                "{route:?}"
            );
        }
        assert!(section_playback_range(&[1, 2, 5, 6], &section)
            .unwrap_err()
            .contains("unreachable"));
    }

    fn positions(ids: &[usize], events: &[usize]) -> Vec<u8> {
        format!(
            "<score><elements>{}</elements><events>{}</events></score>",
            ids.iter()
                .map(|id| format!("<element id=\"{id}\"/>"))
                .collect::<String>(),
            events
                .iter()
                .enumerate()
                .map(|(at, id)| format!("<event elid=\"{id}\" position=\"{}\"/>", at * 1000))
                .collect::<String>()
        )
        .into_bytes()
    }

    #[test]
    fn validates_native_mapping_before_using_played_coordinates() {
        let source = score("<Measure/><Measure><voice><RehearsalMark><text>B</text></RehearsalMark></voice></Measure><Measure/>");
        let mut expanded = score(&"<Measure/>".repeat(5));
        let sections = source.rehearsal_sections().unwrap();
        let native = positions(&[0, 1, 2], &[0, 1, 0, 1, 2]);
        assert_eq!(
            source
                .prepare_playback(&mut expanded, &native, &sections)
                .unwrap(),
            [1..=3, 2..=4]
        );
        for invalid in [
            positions(&[0, 1], &[0, 1, 0, 1, 2]),
            positions(&[0, 2, 1], &[0, 1, 0, 1, 2]),
            positions(&[0, 1, 2], &[0, 1, 0, 1, 3]),
            positions(&[0, 1, 2], &[0, 1, 2]),
            b"<score><elements/><events/></score>".to_vec(),
            b"not XML".to_vec(),
            native
                .iter()
                .copied()
                .chain(b"garbage".iter().copied())
                .collect(),
        ] {
            assert!(source
                .prepare_playback(&mut expanded, &invalid, &sections)
                .is_err());
        }
        assert!(source
            .prepare_playback(&mut score("<Measure/>"), &native, &sections)
            .is_err());
        assert!(consistent_measure_count(&score("")).is_err());
    }

    #[test]
    fn collapsed_rest_layout_does_not_change_written_section_metadata_or_mapping() {
        let source = score("<Measure/><Measure len=\"8/4\"><multiMeasureRest>2</multiMeasureRest></Measure><Measure/><Measure><voice><RehearsalMark><text>A</text></RehearsalMark></voice></Measure>");
        let sections = source.rehearsal_sections().unwrap();
        assert_eq!((sections[0].start_measure, sections[0].end_measure), (1, 2));
        assert_eq!((sections[1].start_measure, sections[1].end_measure), (3, 3));
        let playback = source.playback_source();
        assert!(
            consistent_measure_count(&source).is_err(),
            "cached layout bars must be removed before playback mapping"
        );
        assert_eq!(playback.rehearsal_sections().unwrap(), sections);
        assert_eq!(
            children(playback.master().get_child("Staff").unwrap(), "Measure").count(),
            3
        );
        assert_eq!(
            child_text(
                playback.master().get_child("Style").unwrap(),
                "createMultiMeasureRests"
            )
            .as_deref(),
            Some("0")
        );
        assert_eq!(
            children(source.master().get_child("Staff").unwrap(), "Measure").count(),
            4
        );
        assert_eq!(
            playback
                .prepare_playback(
                    &mut playback.clone(),
                    &positions(&[0, 1, 2], &[0, 1, 2]),
                    &sections
                )
                .unwrap(),
            [1..=3, 3..=3]
        );
    }

    #[test]
    fn expanded_jumps_restore_inherited_and_default_tempo_before_local_changes() {
        let source = score("<Measure/><Measure/><Measure><voice><Tempo><tempo>3</tempo><followText>1</followText><eid>original</eid></Tempo></voice></Measure><Measure/>");
        // Native expansion has omitted the default inherited tempo at the jump back to bar 2.
        let mut expanded = score("<Measure/><Measure/><Measure><voice><Tempo><tempo>3</tempo></Tempo></voice></Measure><Measure/><Measure/><Measure><voice><Tempo><tempo>3</tempo></Tempo></voice></Measure>");
        source
            .prepare_playback(
                &mut expanded,
                &positions(&[0, 1, 2, 3], &[0, 1, 2, 3, 1, 2]),
                &[],
            )
            .unwrap();
        let bars: Vec<_> =
            children(expanded.master().get_child("Staff").unwrap(), "Measure").collect();
        assert_eq!(
            child_text(
                bars[4]
                    .get_child("voice")
                    .unwrap()
                    .get_child("Tempo")
                    .unwrap(),
                "tempo"
            )
            .as_deref(),
            Some("2")
        );
        assert_eq!(
            child_text(
                bars[5]
                    .get_child("voice")
                    .unwrap()
                    .get_child("Tempo")
                    .unwrap(),
                "tempo"
            )
            .as_deref(),
            Some("3")
        );
        let source = score("<Measure><voice><Tempo><tempo>1</tempo><followText>1</followText><eid>original</eid></Tempo></voice></Measure><Measure/><Measure><voice><Tempo><tempo>3</tempo></Tempo></voice></Measure>");
        let mut expanded = score("<Measure/><Measure/><Measure/><Measure><voice><Tempo><tempo>4</tempo></Tempo></voice></Measure><Measure/>");
        source
            .prepare_playback(&mut expanded, &positions(&[0, 1, 2], &[0, 1, 2, 1, 2]), &[])
            .unwrap();
        let bar = children(expanded.master().get_child("Staff").unwrap(), "Measure")
            .nth(3)
            .unwrap();
        let tempos: Vec<_> = children(bar.get_child("voice").unwrap(), "Tempo").collect();
        assert_eq!(
            tempos
                .iter()
                .map(|t| child_text(t, "tempo").unwrap())
                .collect::<Vec<_>>(),
            ["1", "4"]
        );
        assert_eq!(child_text(tempos[0], "followText").as_deref(), Some("0"));
        assert!(tempos[0].get_child("eid").is_none());
    }
}
