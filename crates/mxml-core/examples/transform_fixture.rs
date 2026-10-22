//! Migration test driver: exercises the engine before output schema validation.
//! Some legacy algorithm fixtures deliberately contain schema-invalid annotations.
use mxml_core::{merge_score, read_score, serialize_xml, split_score, Result, ScoreError};
use std::{io::Write, path::Path};

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        return Err(ScoreError::new("Expected operation and fixture path"));
    }
    let score = read_score(Path::new(&args[1]))?;
    let root = match args[0].as_str() {
        "split" => {
            let result = split_score(
                &score,
                args.get(2).map(String::as_str).unwrap_or("chords"),
                None,
            )?;
            for warning in result.warnings {
                eprintln!("mxml: warning: {warning}");
            }
            result.root
        }
        "merge" => {
            let groups: Vec<Vec<String>> = args[2..]
                .iter()
                .map(|group| group.split('+').map(str::to_owned).collect())
                .collect();
            merge_score(&score, &groups)?
        }
        _ => return Err(ScoreError::new("Unknown fixture operation")),
    };
    std::io::stdout().write_all(&serialize_xml(&root)?)?;
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("mxml: {error}");
        std::process::exit(2);
    }
}
