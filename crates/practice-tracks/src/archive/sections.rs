use super::*;
use crate::PracticeSection;

impl ScoreArchive {
    pub fn rehearsal_sections(&self) -> Result<Vec<PracticeSection>> {
        let mut marks: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        let mut measure_count = None;
        let mut consistent = true;
        for staff in children(self.master(), "Staff") {
            let measures: Vec<_> = children(staff, "Measure").collect();
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

    pub(crate) fn region(&self, section: &PracticeSection) -> Value {
        json!({
            "start": {"measure": section.start_measure, "staff": 1},
            "end": {"measure": section.end_measure, "staff": children(self.master(), "Staff").count()}
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

    pub(crate) fn verify_region(&self, source: &Self, section: &PracticeSection) -> Result<()> {
        self.verify_normalized(source)?;
        let expected = section.end_measure - section.start_measure + 1;
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

    pub(crate) fn prepare_section(&mut self, source: &Self, section: &PracticeSection) {
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
        let mut active = None;
        for staff in children(source.master(), "Staff") {
            for (index, measure) in children(staff, "Measure")
                .enumerate()
                .take(section.start_measure - 1)
            {
                visit(measure, &mut |node| {
                    if node.name == "Tempo" && active.as_ref().is_none_or(|(at, _)| index >= *at) {
                        active = Some((index, node.clone()));
                    }
                });
            }
        }
        if let Some((_, tempo)) = active {
            // Native region export drops tempo annotations before the selected measures.
            let staff = self
                .master_mut()
                .children
                .iter_mut()
                .filter_map(XMLNode::as_mut_element)
                .find(|node| node.name == "Staff")
                .unwrap();
            let measure = staff.get_mut_child("Measure").unwrap();
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
    }
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
        region.verify_region(&source, section).unwrap();
        region.prepare_section(&source, section);
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
        region.verify_region(&source, section).unwrap();
        region.prepare_section(&source, section);
        let mut types = vec![];
        visit(region.master(), &mut |node| {
            if node.name == "Spanner" {
                types.push(node.attributes["type"].clone());
            }
        });
        assert_eq!(types, ["Tie", "Slur", "Tie"]);
    }
}
