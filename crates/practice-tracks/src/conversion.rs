//! MusicXML preparation shared by arranging and comparison.
use crate::{renderer, Result, ScoreArchive};
use std::{ffi::OsString, fs::File, io::Read, path::Path, sync::atomic::AtomicBool};

pub fn convert_musicxml(bytes: &[u8], context: &str, executable: Option<&Path>) -> Result<Vec<u8>> {
    let archive = ScoreArchive::read_bytes(bytes, context)?.visible_score()?;
    let executable = renderer::find_executable(executable)?;
    let cancelled = AtomicBool::new(false);
    renderer::check_version(&executable, &cancelled)?;
    let temporary = tempfile::tempdir().map_err(|error| error.to_string())?;
    let input = temporary.path().join("visible.mscz");
    let output = temporary.path().join("converted.musicxml");
    archive.write(&input)?;
    renderer::run(
        &executable,
        &[
            OsString::from("-o"),
            output.as_os_str().into(),
            input.as_os_str().into(),
        ],
        renderer::CONVERSION_TIMEOUT,
        &cancelled,
    )?;
    let mut converted = Vec::new();
    File::open(&output)
        .map_err(|error| format!("MuseScore did not produce MusicXML: {error}"))?
        .take(scorewing_core::MAX_XML_BYTES as u64 + 1)
        .read_to_end(&mut converted)
        .map_err(|error| format!("Cannot read converted MusicXML: {error}"))?;
    scorewing_core::read_score_bytes(&converted, "converted.musicxml")
        .map_err(|error| format!("Converted MusicXML cannot be arranged: {error}"))?;
    Ok(converted)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    #[test]
    fn rejects_failed_invalid_and_multistaff_exports_and_cleans_temporary_files() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("fake-converter");
        fs::write(&executable, "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'MuseScore Studio 4.7.0'; exit; fi\nprintf '%s' \"$3\" > \"$(dirname \"$0\")/input-path\"\nif [ -f \"$(dirname \"$0\")/fail\" ]; then echo 'conversion failed' >&2; exit 1; fi\ncp \"$(dirname \"$0\")/result.xml\" \"$2\"\n").unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        let source = br#"<museScore version="4.70"><Score><Part><Staff/><Instrument id="piano"/></Part><Staff id="1"/></Score></museScore>"#;
        fs::write(directory.path().join("fail"), "").unwrap();
        assert!(convert_musicxml(source, "score.mscx", Some(&executable))
            .unwrap_err()
            .contains("conversion failed"));
        assert!(
            !Path::new(&fs::read_to_string(directory.path().join("input-path")).unwrap()).exists()
        );
        fs::remove_file(directory.path().join("fail")).unwrap();
        fs::write(directory.path().join("result.xml"), "invalid").unwrap();
        assert!(convert_musicxml(source, "score.mscx", Some(&executable))
            .unwrap_err()
            .contains("Converted MusicXML"));
        fs::write(directory.path().join("result.xml"), r#"<score-partwise version="4.0"><part-list><score-part id="P1"><part-name>Piano</part-name></score-part></part-list><part id="P1"><measure number="1"><attributes><divisions>1</divisions><staves>2</staves></attributes></measure></part></score-partwise>"#).unwrap();
        assert!(convert_musicxml(source, "score.mscx", Some(&executable))
            .unwrap_err()
            .contains("staff"));
        assert!(
            !Path::new(&fs::read_to_string(directory.path().join("input-path")).unwrap()).exists()
        );
    }
}
