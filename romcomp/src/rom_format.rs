use bitflags::bitflags;

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

        /// the file format flags
        const FILE_FORMATS = 0b11111111;

        /// either a bin / cue combination, or an iso
        const PSX = 0b100000000;
        /// either a bin / cue combination, or an iso
        const PS2 = 0b1000000000;
        /// an iso
        const PSP = 0b10000000000;
    }
}
