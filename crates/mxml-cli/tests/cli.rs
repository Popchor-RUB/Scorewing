use std::{fs, process::Command};

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mxml"))
}

const SCORE: &str = r#"<score-partwise version="4.0"><work><work-title>CLI fixture</work-title></work><part-list><score-part id="P1"><part-name>Soprano</part-name></score-part><score-part id="P2"><part-name>Alto</part-name></score-part></part-list><part id="P1"><measure number="1"><attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef></attributes><note><pitch><step>E</step><octave>4</octave></pitch><duration>4</duration><type>whole</type></note><note><chord/><pitch><step>C</step><octave>4</octave></pitch><duration>4</duration><type>whole</type></note></measure></part><part id="P2"><measure number="1"><attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef></attributes><note><pitch><step>A</step><octave>4</octave></pitch><duration>4</duration><type>whole</type></note></measure></part></score-partwise>"#;

#[test]
fn help_version_and_error_contract() {
    let help = cli().arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("split"));
    let version = cli().arg("--version").output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        "mxml 0.1.0"
    );
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("invalid.xml");
    let output = dir.path().join("out.xml");
    fs::write(&source, "not xml").unwrap();
    let result = cli()
        .arg("split")
        .arg(source)
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("mxml:"));
    assert!(!output.exists());
}

#[test]
fn split_merge_compressed_roundtrip_and_output_protection() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.musicxml");
    let split = dir.path().join("split.mxl");
    let merged = dir.path().join("merged.musicxml");
    fs::write(&input, SCORE).unwrap();
    assert!(cli()
        .arg("split")
        .arg(&input)
        .arg("-o")
        .arg(&split)
        .args(["--part", "Soprano"])
        .status()
        .unwrap()
        .success());
    let score = mxml_core::read_score(&split).unwrap();
    assert_eq!(
        score.parts.iter().map(|p| p.name()).collect::<Vec<_>>(),
        ["Soprano", "Soprano 1", "Soprano 2", "Alto"]
    );
    assert!(cli()
        .arg("merge")
        .arg(&split)
        .arg("-o")
        .arg(&merged)
        .args(["--pair", "2+3+4"])
        .status()
        .unwrap()
        .success());
    assert_eq!(mxml_core::read_score(&merged).unwrap().parts.len(), 2);
    let before = fs::read(&merged).unwrap();
    assert!(!cli()
        .arg("split")
        .arg(&input)
        .arg("-o")
        .arg(&merged)
        .status()
        .unwrap()
        .success());
    assert_eq!(fs::read(&merged).unwrap(), before);
    assert!(!cli()
        .arg("split")
        .arg(&input)
        .arg("-o")
        .arg(&input)
        .arg("--force")
        .status()
        .unwrap()
        .success());
    assert_eq!(fs::read_to_string(&input).unwrap(), SCORE);
}

#[test]
fn duplicate_names_require_positions_and_bad_selections_publish_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("duplicate.xml");
    let output = dir.path().join("result.xml");
    fs::write(&input, SCORE.replace("Alto", "Soprano")).unwrap();
    for selector in ["Soprano", "0", "99", "Missing"] {
        let result = cli()
            .arg("split")
            .arg(&input)
            .arg("-o")
            .arg(&output)
            .args(["--part", selector])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2));
        assert!(!output.exists());
    }
    assert!(cli()
        .arg("split")
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .args(["--part", "1"])
        .status()
        .unwrap()
        .success());
    assert_eq!(mxml_core::read_score(&output).unwrap().parts.len(), 4);
}
