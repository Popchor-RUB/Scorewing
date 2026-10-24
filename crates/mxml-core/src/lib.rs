pub mod diff;
pub mod io;
pub mod merge;
pub mod model;
pub mod split;
pub mod xml;

pub use diff::{diff_score, DiffResult};
pub use io::*;
pub use merge::{merge_score, merge_score_with_stems};
pub use model::*;
pub use split::{
    split_plan, split_score, split_score_with_fallbacks, split_score_with_options, SplitFallback,
    SplitLane, SplitOptions, SplitResult,
};
pub use xmltree::{Element, XMLNode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreError(pub String);

impl ScoreError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}
impl std::fmt::Display for ScoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ScoreError {}
impl From<std::io::Error> for ScoreError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}
impl From<zip::result::ZipError> for ScoreError {
    fn from(error: zip::result::ZipError) -> Self {
        Self(format!("Invalid or unsupported MXL archive: {error}"))
    }
}
pub type Result<T> = std::result::Result<T, ScoreError>;

pub(crate) fn check_errors(errors: Vec<ScoreError>) -> Result<()> {
    let mut messages = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for error in &errors {
        for message in error.0.lines() {
            if seen.insert(message) {
                messages.push(message);
            }
        }
    }
    if messages.is_empty() {
        Ok(())
    } else {
        Err(ScoreError::new(messages.join("\n")))
    }
}
