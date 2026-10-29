//! MuseScore practice mixes. Scores are prepared in copies and rendered by MuseScore 4.7.
pub mod archive;
pub mod catalog;
mod conversion;
mod preferences;
pub mod renderer;
pub mod service;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub use archive::ScoreArchive;
pub use catalog::SoundCatalog;
pub use conversion::convert_musicxml;
pub use renderer::environment;
pub use service::export_tracks;

pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresetId {
    pub bank: u16,
    pub program: u8,
}

#[derive(Clone, Debug, Serialize)]
pub struct Preset {
    pub bank: u16,
    pub program: u8,
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PracticePart {
    pub key: String,
    pub name: String,
    pub visible: bool,
    pub original_instruments: Vec<String>,
    pub existing_preset: Option<PresetId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PracticeSection {
    pub key: String,
    pub name: String,
    /// One-based, inclusive written-measure positions (including pickups).
    pub start_measure: usize,
    pub end_measure: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PartSettings {
    pub key: String,
    pub foreground_sound: Option<PresetId>,
    pub background_sound: Option<PresetId>,
    pub include_in_background: bool,
    pub create_track: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PracticeOptions {
    pub parts: Vec<PartSettings>,
    #[serde(default = "include_whole_song")]
    pub include_whole_song: bool,
    #[serde(default)]
    pub section_keys: Vec<String>,
    pub background_reduction_db: f64,
    pub metronome_enabled: bool,
    pub metronome_db: f64,
}

fn include_whole_song() -> bool {
    true
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RendererConfig {
    pub executable: Option<PathBuf>,
    pub soundfont: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct RendererEnvironment {
    pub executable: Option<String>,
    pub soundfont: Option<String>,
    pub version: Option<String>,
    pub presets: Vec<Preset>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub stage: String,
    pub completed: usize,
    pub running: usize,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct CompletedTrack {
    pub key: String,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct ExportReport {
    pub directory: String,
    pub completed: Vec<CompletedTrack>,
    pub error: Option<String>,
    pub cancelled: bool,
}
