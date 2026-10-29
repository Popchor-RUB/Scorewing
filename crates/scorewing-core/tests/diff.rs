use scorewing_core::{
    diff_score, read_score, read_score_bytes, validate_score, write_score,
    xml::{child, children, text, visit, visit_mut},
    DiffResult, Element, Score,
};

fn note(step: &str, duration: u32, voice: &str, extra: &str) -> String {
    format!("<note><pitch><step>{step}</step><octave>4</octave></pitch><duration>{duration}</duration><voice>{voice}</voice><type>{}</type>{extra}</note>", if duration == 2 { "half" } else { "quarter" })
}

fn document(definitions: &str, parts: &str) -> Score {
    read_score_bytes(format!("<score-partwise version=\"4.0\"><part-list>{definitions}</part-list>{parts}</score-partwise>").as_bytes(), "diff test").unwrap()
}

fn measures(body: &str) -> Score {
    document(
        "<score-part id=\"P1\"><part-name>Choir</part-name></score-part>",
        &format!("<part id=\"P1\">{body}</part>"),
    )
}

fn score(body: &str) -> Score {
    measures(&format!(
        "<measure number=\"1\"><attributes><divisions>1</divisions></attributes>{body}</measure>"
    ))
}

fn compare(old: &Score, new: &Score) -> DiffResult {
    let result = diff_score(old, new).unwrap();
    validate_score(&result.root).unwrap();
    let mut actual = result.root.clone();
    let mut expected = new.root.clone();
    for root in [&mut actual, &mut expected] {
        visit_mut(root, &mut |node| {
            node.attributes.remove("color");
        });
    }
    assert_eq!(
        actual, expected,
        "comparison must only change color attributes"
    );
    result
}

fn colored(root: &Element, name: &str) -> usize {
    let mut count = 0;
    visit(root, &mut |node| {
        count += usize::from(
            node.name == name && node.attributes.get("color").is_some_and(|c| c == "#FF0000"),
        );
    });
    count
}

fn note_colors(root: &Element) -> Vec<bool> {
    let mut colors = Vec::new();
    visit(root, &mut |node| {
        if node.name == "note" {
            colors.push(node.attributes.get("color").is_some_and(|c| c == "#FF0000"));
        }
    });
    colors
}

#[test]
fn changed_pitch_length_voice_and_rests_mark_only_the_affected_note() {
    let original = note("C", 1, "1", "");
    for changed in [
        note("D", 1, "1", ""),
        note("C", 2, "1", ""),
        note("C", 1, "2", ""),
        "<note><rest/><duration>1</duration><voice>1</voice><type>quarter</type></note>".into(),
    ] {
        let prefix = note("E", 1, "1", "");
        let suffix = note("G", 1, "1", "");
        let result = compare(
            &score(&format!("{prefix}{original}{suffix}")),
            &score(&format!("{prefix}{changed}{suffix}")),
        );
        assert_eq!(note_colors(&result.root), [false, true, false]);
    }
}

#[test]
fn lyric_dynamics_and_harmony_changes_color_the_objects() {
    let body = |word: &str, dynamic: &str, chord: &str| {
        format!(
        "<direction><direction-type><dynamics><{dynamic}/></dynamics></direction-type></direction><harmony><root><root-step>{chord}</root-step></root><kind>major</kind></harmony>{}",
        note("C", 1, "1", &format!("<lyric number=\"1\"><syllabic>single</syllabic><text>{word}</text></lyric>")))
    };
    let result = compare(
        &score(&body("old", "p", "C")),
        &score(&body("new", "f", "D")),
    );
    assert_eq!(colored(&result.root, "note"), 0);
    for name in ["lyric", "dynamics", "harmony"] {
        assert_eq!(colored(&result.root, name), 1);
    }
    assert!(result.warnings.is_empty());
}

#[test]
fn deleting_lyrics_or_symbols_marks_surviving_music_and_warns() {
    for extra in [
        "<direction><direction-type><dynamics><p/></dynamics></direction-type></direction>",
        "<harmony><root><root-step>C</root-step></root><kind>major</kind></harmony>",
    ] {
        let result = compare(
            &score(&format!("{extra}{}", note("C", 1, "1", ""))),
            &score(&note("C", 1, "1", "")),
        );
        assert_eq!(colored(&result.root, "note"), 1);
        assert!(!result.warnings.is_empty());
    }
    let result = compare(
        &score(&note("C", 1, "1", "<lyric><text>Ah</text></lyric>")),
        &score(&note("C", 1, "1", "")),
    );
    assert_eq!(colored(&result.root, "note"), 1);
    assert!(result.warnings.iter().any(|w| w.contains("Deleted lyrics")));
}

#[test]
fn enharmonic_microtonal_and_divisions_equivalence_ignore_engraving_and_ids() {
    let old = score(&note("C", 1, "1", "").replace("</step>", "</step><alter>0.5</alter>"));
    let mut new = score(&note("D", 2, "1", "").replace("</step>", "</step><alter>-1.5</alter>"));
    // One half-note duration in two divisions is one quarter-note duration.
    let source = scorewing_core::serialize_xml(&new.root).unwrap();
    let source = String::from_utf8(source)
        .unwrap()
        .replace("<divisions>1</divisions>", "<divisions>2</divisions>")
        .replace("<part id=\"P1\">", "<part id=\"Reexported\">")
        .replace("<score-part id=\"P1\">", "<score-part id=\"Reexported\">")
        .replace(
            "<note>",
            "<note color=\"#008000\" default-x=\"32\" id=\"note123\">",
        )
        .replace("<voice>", "<!-- exporter comment --><voice>");
    new = read_score_bytes(source.as_bytes(), "new").unwrap();
    let result = compare(&old, &new);
    assert_eq!(colored(&result.root, "note"), 0);
    assert!(result.warnings.is_empty());
    assert_eq!(
        child(
            child(child(&result.root, "part").unwrap(), "measure").unwrap(),
            "note"
        )
        .unwrap()
        .attributes["color"],
        "#008000"
    );
}

#[test]
fn sounding_pitch_includes_instrument_transposition() {
    let old = score(&note("C", 1, "1", ""));
    let new = measures(&format!("<measure number=\"1\"><attributes><divisions>1</divisions><transpose><diatonic>-1</diatonic><chromatic>-2</chromatic></transpose></attributes>{}</measure>", note("D",1,"1","")));
    assert_eq!(colored(&compare(&old, &new).root, "note"), 0);
    let changed = measures(&format!("<measure number=\"1\"><attributes><divisions>1</divisions><transpose><diatonic>-1</diatonic><chromatic>-2</chromatic></transpose></attributes>{}</measure>", note("C",1,"1","")));
    assert_eq!(colored(&compare(&old, &changed).root, "note"), 1);
}

fn connection(kind: &str, number: &str, end: &str) -> String {
    format!("<notations><{kind} type=\"{end}\" number=\"{number}\"/></notations>")
}

#[test]
fn added_and_removed_ties_or_slurs_mark_both_endpoints() {
    for kind in ["tied", "slur"] {
        let plain = score(&format!(
            "{}{}",
            note("C", 1, "1", ""),
            note("C", 1, "1", "")
        ));
        let connected = score(&format!(
            "{}{}",
            note("C", 1, "1", &connection(kind, "1", "start")),
            note("C", 1, "1", &connection(kind, "1", "stop"))
        ));
        let added = compare(&plain, &connected);
        assert_eq!(note_colors(&added.root), [true, true]);
        assert_eq!(colored(&added.root, kind), 2);
        let removed = compare(&connected, &plain);
        assert_eq!(note_colors(&removed.root), [true, true]);
        assert!(!removed.warnings.is_empty());
        let renumbered = score(&format!(
            "{}{}",
            note("C", 1, "1", &connection(kind, "7", "start")),
            note("C", 1, "1", &connection(kind, "7", "stop"))
        ));
        let result = compare(&connected, &renumbered);
        assert_eq!(colored(&result.root, "note"), 0);
        assert!(result.warnings.is_empty());
    }
}

#[test]
fn changed_slur_endpoint_and_inserted_note_preserve_connection_matching() {
    let old = score(&format!(
        "{}{}{}",
        note("C", 1, "1", &connection("slur", "1", "start")),
        note("D", 1, "1", &connection("slur", "1", "stop")),
        note("E", 1, "1", "")
    ));
    let new = score(&format!(
        "{}{}{}",
        note("C", 1, "1", &connection("slur", "9", "start")),
        note("D", 1, "1", ""),
        note("E", 1, "1", &connection("slur", "9", "stop"))
    ));
    assert_eq!(note_colors(&compare(&old, &new).root), [true, true, true]);
    let inserted = score(&format!(
        "{}{}{}{}",
        note("C", 1, "1", &connection("slur", "9", "start")),
        note("F", 1, "1", ""),
        note("D", 1, "1", &connection("slur", "9", "stop")),
        note("E", 1, "1", "")
    ));
    let result = compare(&old, &inserted);
    assert_eq!(note_colors(&result.root), [false, true, false, false]);
    assert_eq!(colored(&result.root, "slur"), 0);
}

#[test]
fn playback_tie_and_notation_tied_describe_the_same_connection() {
    let old = score(
        &format!(
            "{}{}",
            note("C", 1, "1", "<tie type=\"start\"/>"),
            note("C", 1, "1", "<tie type=\"stop\"/>")
        )
        .replace("<type>quarter</type><tie", "<tie")
        .replace("type=\"start\"/>", "type=\"start\"/><type>quarter</type>")
        .replace("type=\"stop\"/>", "type=\"stop\"/><type>quarter</type>"),
    );
    let new = score(&format!(
        "{}{}",
        note("C", 1, "1", &connection("tied", "1", "start")),
        note("C", 1, "1", &connection("tied", "1", "stop"))
    ));
    assert_eq!(colored(&compare(&old, &new).root, "note"), 0);
}

#[test]
fn note_and_measure_insertions_and_deletions_do_not_cascade() {
    let c = note("C", 1, "1", "");
    let d = note("D", 1, "1", "");
    let e = note("E", 1, "1", "");
    let f = note("F", 1, "1", "");
    let old = score(&format!("{c}{d}{e}"));
    let new = score(&format!("{c}{f}{d}{e}"));
    assert_eq!(
        note_colors(&compare(&old, &new).root),
        [false, true, false, false]
    );
    let removed = compare(&new, &old);
    assert_eq!(note_colors(&removed.root), [false, true, false]);
    assert!(removed.warnings.iter().any(|w| w.contains("Deleted 1")));
    let old=measures(&format!("<measure number=\"1\">{c}</measure><measure number=\"2\">{d}</measure><measure number=\"3\">{e}</measure>"));
    let new=measures(&format!("<measure number=\"1\">{c}</measure><measure number=\"2\">{f}</measure><measure number=\"3\">{d}</measure><measure number=\"4\">{e}</measure>"));
    assert_eq!(
        note_colors(&compare(&old, &new).root),
        [false, true, false, false]
    );
    assert_eq!(note_colors(&compare(&new, &old).root), [false, true, false]);
}

#[test]
fn polyphony_and_chord_serialization_order_do_not_create_changes() {
    let c = note("C", 1, "1", "");
    let e = note("E", 1, "2", "");
    let backup = "<backup><duration>1</duration></backup>";
    let old = score(&format!("{c}{backup}{e}"));
    let reordered = score(&format!("{e}{backup}{c}"));
    assert_eq!(colored(&compare(&old, &reordered).root, "note"), 0);
    let moved = score(&format!("{e}{backup}{}", note("C", 1, "3", "")));
    assert_eq!(note_colors(&compare(&old, &moved).root), [false, true]);
    let chord = |a: &str, b: &str| {
        format!(
            "{}{}",
            note(a, 1, "1", ""),
            note(b, 1, "1", "").replace("<note>", "<note><chord/>")
        )
    };
    assert_eq!(
        colored(
            &compare(&score(&chord("C", "E")), &score(&chord("E", "C"))).root,
            "note"
        ),
        0
    );
}

#[test]
fn dynamics_offsets_wedges_and_empty_measures_are_compared() {
    let a="<direction><direction-type><dynamics><p/></dynamics></direction-type><offset>0</offset></direction>";
    let b = a.replace("<offset>0", "<offset>1");
    assert_eq!(
        colored(
            &compare(
                &score(&format!("{a}{}", note("C", 2, "1", ""))),
                &score(&format!("{b}{}", note("C", 2, "1", "")))
            )
            .root,
            "dynamics"
        ),
        1
    );
    let wedge = |kind| {
        format!(
            "<direction><direction-type><wedge type=\"{kind}\"/></direction-type></direction>{}",
            note("C", 1, "1", "")
        )
    };
    assert_eq!(
        colored(
            &compare(&score(&wedge("crescendo")), &score(&wedge("diminuendo"))).root,
            "wedge"
        ),
        1
    );
    assert_eq!(colored(&compare(&score(a), &score(a)).root, "dynamics"), 0);
}

#[test]
fn grace_note_length_changes_are_visible() {
    let grace = |kind| {
        format!("<note><grace/><pitch><step>D</step><octave>4</octave></pitch><voice>1</voice><type>{kind}</type></note>{}",note("C",1,"1",""))
    };
    assert_eq!(
        note_colors(&compare(&score(&grace("eighth")), &score(&grace("16th"))).root),
        [true, false]
    );
}

#[test]
fn parts_match_by_unique_names_then_ids_and_ambiguous_names_fail() {
    let def =
        |id, name| format!("<score-part id=\"{id}\"><part-name>{name}</part-name></score-part>");
    let part = |id, step| {
        format!(
            "<part id=\"{id}\"><measure number=\"1\">{}</measure></part>",
            note(step, 1, "1", "")
        )
    };
    let old = document(
        &format!("{}{}", def("P1", "Soprano"), def("P2", "Alto")),
        &format!("{}{}", part("P1", "C"), part("P2", "D")),
    );
    let new = document(
        &format!("{}{}", def("NEW1", "Alto"), def("NEW2", "Soprano")),
        &format!("{}{}", part("NEW1", "D"), part("NEW2", "C")),
    );
    assert_eq!(colored(&compare(&old, &new).root, "note"), 0);
    let deleted = document(&def("P2", "Alto"), &part("P2", "D"));
    let result = compare(&old, &deleted);
    assert_eq!(colored(&result.root, "note"), 1);
    assert!(result
        .warnings
        .iter()
        .any(|w| w.contains("Deleted instrument")));
    assert_eq!(note_colors(&compare(&deleted, &old).root), [true, false]);
    let duplicate_old = document(
        &format!("{}{}", def("A", "Choir"), def("B", "Choir")),
        &format!("{}{}", part("A", "C"), part("B", "D")),
    );
    let duplicate_new = document(
        &format!("{}{}", def("X", "Choir"), def("Y", "Choir")),
        &format!("{}{}", part("X", "C"), part("Y", "D")),
    );
    assert!(diff_score(&duplicate_old, &duplicate_new)
        .err()
        .unwrap()
        .to_string()
        .contains("unambiguously"));
}

#[test]
fn repeated_passages_warn_and_export_roundtrips_plain_and_compressed() {
    let old = score(&format!(
        "{}{}{}",
        note("C", 1, "1", ""),
        note("C", 1, "1", ""),
        note("D", 1, "1", "")
    ));
    let new = score(&format!(
        "{}{}",
        note("C", 1, "1", ""),
        note("D", 1, "1", "")
    ));
    let result = compare(&old, &new);
    assert!(result.warnings.iter().any(|w| w.contains("ambiguous")));
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("new.musicxml");
    let input_bytes = scorewing_core::serialize_xml(&new.root).unwrap();
    std::fs::write(&input, &input_bytes).unwrap();
    for ext in ["musicxml", "mxl"] {
        let output = dir.path().join(format!("diff.{ext}"));
        write_score(&result.root, &output, &input, false).unwrap();
        assert_eq!(
            note_colors(&read_score(&output).unwrap().root),
            note_colors(&result.root)
        );
    }
    assert_eq!(std::fs::read(&input).unwrap(), input_bytes);
    assert_eq!(
        children(child(&result.root, "part").unwrap(), "measure").count(),
        1
    );
    assert_eq!(text(child(&result.root, "part-list").unwrap()).trim(), "");
}

#[test]
fn deletions_with_no_surviving_notes_mark_a_symbol_or_explain_the_limit() {
    let old = score(&note("C", 1, "1", ""));
    let new =
        score("<direction><direction-type><dynamics><p/></dynamics></direction-type></direction>");
    let result = compare(&old, &new);
    assert_eq!(colored(&result.root, "dynamics"), 1);
    assert!(result
        .warnings
        .iter()
        .any(|w| w.contains("nearest surviving music")));
    let empty = compare(&old, &score(""));
    assert!(empty
        .warnings
        .iter()
        .any(|w| w.contains("no surviving music")));
}

#[test]
fn a_let_ring_tie_is_different_from_an_unfinished_tie() {
    let old = score(&note("C", 1, "1", &connection("tied", "1", "start")));
    let new = score(&note("C", 1, "1", &connection("tied", "1", "let-ring")));
    let result = compare(&old, &new);
    assert_eq!(colored(&result.root, "note"), 1);
    assert_eq!(colored(&result.root, "tied"), 1);
}
