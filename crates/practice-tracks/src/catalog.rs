use crate::{Preset, PresetId, Result};
use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Clone, Debug)]
pub struct SoundCatalog {
    pub presets: Vec<Preset>,
}

impl SoundCatalog {
    pub fn read(path: &Path) -> Result<Self> {
        Self::read_inner(path)
            .map_err(|e| format!("Cannot read MS Basic SoundFont {}: {e}", path.display()))
    }

    fn read_inner(path: &Path) -> Result<Self> {
        let mut file = File::open(path).map_err(|e| e.to_string())?;
        let size = file.metadata().map_err(|e| e.to_string())?.len();
        let mut header = [0; 12];
        file.read_exact(&mut header).map_err(|e| e.to_string())?;
        if &header[..4] != b"RIFF" || &header[8..] != b"sfbk" {
            return Err("Not a RIFF SoundFont".into());
        }
        let end = u32::from_le_bytes(header[4..8].try_into().unwrap()) as u64 + 8;
        if end != size || end < 12 {
            return Err("Invalid RIFF size".into());
        }
        let mut presets = None;
        while file.stream_position().map_err(|e| e.to_string())? < end {
            let (tag, start, length, next) = chunk(&mut file, end)?;
            if &tag == b"LIST" {
                if length < 4 {
                    return Err("Invalid LIST chunk".into());
                }
                let mut kind = [0; 4];
                file.read_exact(&mut kind).map_err(|e| e.to_string())?;
                if &kind == b"pdta" {
                    let list_end = start + length;
                    while file.stream_position().map_err(|e| e.to_string())? < list_end {
                        let (tag, _, length, child_next) = chunk(&mut file, list_end)?;
                        if &tag == b"phdr" {
                            if presets.is_some()
                                || length < 76
                                || length % 38 != 0
                                || length > 4 * 1024 * 1024
                            {
                                return Err("Invalid or duplicate preset headers".into());
                            }
                            let mut data = vec![0; length as usize];
                            file.read_exact(&mut data).map_err(|e| e.to_string())?;
                            let mut seen = HashSet::new();
                            let mut parsed = Vec::new();
                            let records: Vec<_> = data.chunks_exact(38).collect();
                            let terminal_name = records.last().unwrap()[..20]
                                .split(|b| *b == 0)
                                .next()
                                .unwrap();
                            // MS Basic SF3 uses an empty terminal name; SF2 conventionally uses EOP.
                            if !terminal_name.is_empty() && terminal_name != b"EOP" {
                                return Err("Missing terminal preset header".into());
                            }
                            for row in &records[..records.len() - 1] {
                                let program = u16::from_le_bytes([row[20], row[21]]);
                                let bank = u16::from_le_bytes([row[22], row[23]]);
                                if program > 127 || bank > 16383 || !seen.insert((bank, program)) {
                                    return Err("Invalid or duplicate preset identifier".into());
                                }
                                let name = String::from_utf8_lossy(
                                    row[..20].split(|b| *b == 0).next().unwrap(),
                                )
                                .into_owned();
                                parsed.push(Preset {
                                    bank,
                                    program: program as u8,
                                    name,
                                });
                            }
                            parsed.sort_by_key(|p| (p.bank, p.program));
                            presets = Some(parsed);
                        }
                        file.seek(SeekFrom::Start(child_next))
                            .map_err(|e| e.to_string())?;
                    }
                }
            }
            file.seek(SeekFrom::Start(next))
                .map_err(|e| e.to_string())?;
        }
        Ok(Self {
            presets: presets.ok_or("Missing phdr preset list")?,
        })
    }

    pub fn get(&self, id: PresetId) -> Result<&Preset> {
        self.presets
            .iter()
            .find(|p| p.bank == id.bank && p.program == id.program)
            .ok_or_else(|| {
                format!(
                    "MS Basic has no preset bank={}, program={}",
                    id.bank, id.program
                )
            })
    }
}

fn chunk(file: &mut File, end: u64) -> Result<([u8; 4], u64, u64, u64)> {
    let position = file.stream_position().map_err(|e| e.to_string())?;
    if end.saturating_sub(position) < 8 {
        return Err("Truncated RIFF chunk header".into());
    }
    let mut header = [0; 8];
    file.read_exact(&mut header).map_err(|e| e.to_string())?;
    let length = u32::from_le_bytes(header[4..].try_into().unwrap()) as u64;
    let start = position + 8;
    let next = start + length + (length & 1);
    if next > end {
        return Err("Truncated RIFF chunk".into());
    }
    Ok((header[..4].try_into().unwrap(), start, length, next))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn font(program: u16) -> Vec<u8> {
        let mut rows = vec![0; 76];
        rows[..5].copy_from_slice(b"Piano");
        rows[20..22].copy_from_slice(&program.to_le_bytes());
        rows[38..41].copy_from_slice(b"EOP");
        let mut pdta = b"pdta".to_vec();
        pdta.extend(b"phdr");
        pdta.extend((rows.len() as u32).to_le_bytes());
        pdta.extend(rows);
        let mut riff = b"sfbkLIST".to_vec();
        riff.extend((pdta.len() as u32).to_le_bytes());
        riff.extend(pdta);
        let mut result = b"RIFF".to_vec();
        result.extend((riff.len() as u32).to_le_bytes());
        result.extend(riff);
        result
    }

    #[test]
    fn bounded_soundfont_headers() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(&font(0)).unwrap();
        let catalog = SoundCatalog::read(file.path()).unwrap();
        assert_eq!(
            catalog
                .get(PresetId {
                    bank: 0,
                    program: 0
                })
                .unwrap()
                .name,
            "Piano"
        );
        assert!(catalog
            .get(PresetId {
                bank: 0,
                program: 1
            })
            .is_err());
        let mut sf3_terminal = font(0);
        sf3_terminal[70..90].fill(0);
        std::fs::write(file.path(), sf3_terminal).unwrap();
        assert_eq!(SoundCatalog::read(file.path()).unwrap().presets.len(), 1);
        let mut truncated = font(0);
        truncated.pop();
        let mut invalid_header = font(0);
        invalid_header[32 + 38..32 + 41].copy_from_slice(b"BAD");
        let mut invalid_chunk = font(0);
        invalid_chunk[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
        for bytes in [font(128), truncated, invalid_header, invalid_chunk] {
            std::fs::write(file.path(), bytes).unwrap();
            assert!(SoundCatalog::read(file.path()).is_err());
        }
    }
}
