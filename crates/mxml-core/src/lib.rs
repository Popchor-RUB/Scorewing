pub mod io;
pub mod merge;
pub mod model;
pub mod split;
pub mod xml;

pub use io::*;
pub use merge::merge_score;
pub use model::*;
pub use split::split_score;
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
