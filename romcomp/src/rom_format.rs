use bitflags::bitflags;
use duct::{cmd, Expression};
use std::path::PathBuf;

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum CompressionTool {
    BitButcher,
    Chdman,
    DolphinTool,
    MaxCSO,
    Rom64,
}

impl CompressionTool {
    pub fn build(&self, input: &PathBuf, output: &PathBuf) -> Expression {
        match self {
            CompressionTool::BitButcher => cmd!("BitButcher", "-e", input.to_str().unwrap(),),
            CompressionTool::Chdman => cmd!(
                "chdman",
                "createcd",
                "-i",
                input.to_str().unwrap(),
                "-o",
                output.to_str().unwrap(),
            ),
            CompressionTool::DolphinTool => cmd!(
                "dolphin-tool",
                "convert",
                "-b",
                "131072",
                "-c",
                "zstd",
                "-f",
                "rvz",
                "-i",
                input.to_str().unwrap(),
                "-l",
                "5",
                "-o",
                output.to_str().unwrap(),
            ),
            CompressionTool::MaxCSO => cmd!("maxcso", input.to_str().unwrap(),),
            CompressionTool::Rom64 => cmd!("rom64", "convert", input.to_str().unwrap(),),
        }
    }
}

// these are the possible rom formats
// some file formats can contain multiple different rom types
// e.g. bin files can contain psx and ps2 roms
// iso files can contain psx, ps2 and psp, and possibly more

bitflags! {
    #[derive(Clone, Copy, Eq, PartialEq)]
    pub struct RomFormat: u16 {
        /// bin file, in combination with a cue or cue.txt file
        const BIN = 0b1;
        /// iso file
        const ISO = 0b10;
        /// Nintendo 64 ROM
        const N64 = 0b100;
        /// Nintendo 64 ROM
        const V64 = 0b1000;
        /// Nintendo 64 ROM
        const Z64 = 0b10000;
        /// Nintendo DS ROM
        const NDS = 0b100000;

        /// the file format flags
        const FILE_FORMATS = 0b11111111;

        /// either a bin / cue combination, or an iso
        const PlayStationX = 0b100000000;
        /// either a bin / cue combination, or an iso
        const PlayStation2 = 0b1000000000;
        /// an iso
        const PlayStationPortable = 0b10000000000;
        /// any of the 3 n64 formats (n64, v64 or z64)
        const Nintendo64 = 0b100000000000;
        /// Nintendo DS
        const NintendoDS = 0b1000000000000;
        /// Nintendo Wii
        const NintendoWii = 0b10000000000000;
    }
}

impl RomFormat {
    pub fn zip(&self) -> bool {
        self.contains(RomFormat::Nintendo64) || self.contains(RomFormat::NintendoDS)
    }

    pub fn compression_tool(&self) -> Option<CompressionTool> {
        if self.contains(RomFormat::PlayStationX) || self.contains(RomFormat::PlayStation2) {
            Some(CompressionTool::Chdman)
        } else if self.contains(RomFormat::PlayStationPortable) {
            Some(CompressionTool::MaxCSO)
        } else if self.contains(RomFormat::Nintendo64) && !self.contains(RomFormat::Z64) {
            Some(CompressionTool::Rom64)
        } else if self.contains(RomFormat::NintendoDS) {
            Some(CompressionTool::BitButcher)
        } else if self.contains(RomFormat::NintendoWii) {
            Some(CompressionTool::DolphinTool)
        } else {
            None
        }
    }
}
