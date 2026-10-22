use clap::{Args, Parser, Subcommand, ValueEnum};
use mxml_core::{
    check_output, merge_score, read_score, split_score, write_score, Score, ScoreError, SplitResult,
};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "mxml",
    version,
    about = "Split and merge choir staves in MusicXML scores."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Args)]
struct Files {
    /// Plain MusicXML or compressed .mxl input
    input: PathBuf,
    /// Output file; extension selects format
    #[arg(short, long)]
    output: PathBuf,
    /// Replace an existing output file
    #[arg(long)]
    force: bool,
}

#[derive(Clone, ValueEnum)]
enum Mode {
    Chords,
    Voices,
}

#[derive(Subcommand)]
enum Command {
    /// Retain original staves and add separate voices below each selected stave
    Split {
        #[command(flatten)]
        files: Files,
        #[arg(long, value_enum, default_value = "chords")]
        mode: Mode,
        /// One-based position or exact part name; repeat to select multiple parts
        #[arg(long = "part")]
        parts: Vec<String>,
    },
    /// Merge explicitly selected staves into numbered voices
    Merge {
        #[command(flatten)]
        files: Files,
        #[arg(long = "pair", required = true, value_name = "STAVE+STAVE[+STAVE]")]
        pairs: Vec<String>,
    },
}

fn select(score: &Score, value: &str) -> Result<String, ScoreError> {
    let value = value.trim();
    if !value.is_empty() && value.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(index) = value.parse::<usize>() {
            if let Some(part) = index.checked_sub(1).and_then(|i| score.parts.get(i)) {
                return Ok(part.id().to_owned());
            }
        }
        return Err(ScoreError(format!("Unknown stave selector {value:?}")));
    }
    let matches: Vec<_> = score.parts.iter().filter(|p| p.name() == value).collect();
    match matches.as_slice() {
        [part] => Ok(part.id().to_owned()),
        [] => Err(ScoreError(format!("Unknown stave {value:?}"))),
        _ => Err(ScoreError(format!(
            "Ambiguous stave name {value:?}; use a one-based position"
        ))),
    }
}

fn run(cli: Cli) -> Result<(), ScoreError> {
    let files = match &cli.command {
        Command::Split { files, .. } | Command::Merge { files, .. } => files,
    };
    check_output(&files.input, &files.output, files.force)?;
    let score = read_score(&files.input)?;
    let result = match &cli.command {
        Command::Split { mode, parts, .. } => {
            let selected = parts
                .iter()
                .map(|s| select(&score, s))
                .collect::<Result<Vec<_>, _>>()?;
            let mode = match mode {
                Mode::Chords => "chords",
                Mode::Voices => "voices",
            };
            split_score(
                &score,
                mode,
                if selected.is_empty() {
                    None
                } else {
                    Some(&selected)
                },
            )?
        }
        Command::Merge { pairs, .. } => {
            let groups = pairs
                .iter()
                .map(|pair| pair.split('+').map(|s| select(&score, s)).collect())
                .collect::<Result<Vec<Vec<String>>, _>>()?;
            SplitResult {
                root: merge_score(&score, &groups)?,
                warnings: Vec::new(),
            }
        }
    };
    write_score(&result.root, &files.output, &files.input, files.force)?;
    for warning in result.warnings {
        eprintln!("mxml: warning: {warning}");
    }
    Ok(())
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("mxml: {error}");
        std::process::exit(2);
    }
}
