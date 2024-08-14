use crate::rom_format::RomFormat;
use std::path::PathBuf;

pub fn guess_file(path: &PathBuf) -> Option<RomFormat> {
    path.extension().and_then(|e| {
        if let Some(e) = e.to_str() {
            match e.to_lowercase().as_ref() {
                "bin" => {
                    if path
                        .parent()
                        .unwrap()
                        .join(format!(
                            "{}.{}",
                            path.file_stem().unwrap().to_str().unwrap(),
                            "cue"
                        ))
                        .is_file()
                        || path
                            .parent()
                            .unwrap()
                            .join(format!(
                                "{}.{}",
                                path.file_stem().unwrap().to_str().unwrap(),
                                "cue.txt"
                            ))
                            .is_file()
                    {
                        Some(RomFormat::PSX | RomFormat::PS2 | RomFormat::BIN)
                    } else {
                        None
                    }
                }
                "iso" => Some(RomFormat::PSX | RomFormat::PS2 | RomFormat::PSP | RomFormat::ISO),
                _ => None,
            }
        } else {
            None
        }
    })
}
