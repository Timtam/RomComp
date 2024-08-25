use crate::rom_format::RomFormat;
use cue::cd::CD;
use std::path::PathBuf;

pub fn guess_file(path: &PathBuf) -> Option<RomFormat> {
    path.file_name().and_then(|e| {
        if let Some(e) = e.to_str() {
            if path.is_file()
                && (e.to_lowercase().ends_with(".cue") || e.to_lowercase().ends_with(".cue.txt"))
            {
                CD::parse_file(path.clone()).ok().and_then(|cue| {
                    if cue.tracks().iter().all(|t| {
                        t.get_filename().to_lowercase().ends_with(".bin")
                            && path.parent().unwrap().join(t.get_filename()).is_file()
                    }) {
                        Some(RomFormat::PlayStationX | RomFormat::PlayStation2 | RomFormat::BIN)
                    } else {
                        None
                    }
                })
            } else if path.is_file() && e.to_lowercase().ends_with(".iso") {
                Some(
                    RomFormat::PlayStationX
                        | RomFormat::PlayStation2
                        | RomFormat::PlayStationPortable
                        | RomFormat::NintendoWii
                        | RomFormat::ISO,
                )
            } else if path.is_file() && e.to_lowercase().ends_with(".n64") {
                Some(RomFormat::N64 | RomFormat::Nintendo64)
            } else if path.is_file() && e.to_lowercase().ends_with(".v64") {
                Some(RomFormat::V64 | RomFormat::Nintendo64)
            } else if path.is_file() && e.to_lowercase().ends_with(".z64") {
                Some(RomFormat::Z64 | RomFormat::Nintendo64)
            } else if path.is_file() && e.to_lowercase().ends_with(".nds") {
                Some(RomFormat::NDS | RomFormat::NintendoDS)
            } else {
                None
            }
        } else {
            None
        }
    })
}
