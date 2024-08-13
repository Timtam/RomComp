use bitflags::bitflags;

bitflags! {
    #[derive(Clone, Copy)]
    pub struct RomFormat: u8 {
        /// either a bin / cue combination, or an iso
        const PSX = 0b1;
        /// either a bin / cue combination, or an iso
        const PS2 = 0b10;
        /// an iso
        const PSP = 0b100;
    }
}
