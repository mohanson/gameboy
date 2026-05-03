#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Term {
    DMG, // Original GameBoy (GameBoy Classic)
    CGB, // GameBoy Color
}

impl std::fmt::Display for Term {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Term::DMG => write!(f, "DMG"),
            Term::CGB => write!(f, "CGB"),
        }
    }
}

pub const CLOCK_FREQUENCY: u32 = 4_194_304;
pub const STEP_TIME: u32 = 16;
pub const STEP_CYCLES: u32 = (STEP_TIME as f64 / (1000_f64 / CLOCK_FREQUENCY as f64)) as u32;

// General Memory Map
// 0000-3FFF   16KB ROM Bank 00          (in cartridge, fixed at bank 00)
// 4000-7FFF   16KB ROM Bank 01..NN      (in cartridge, switchable bank number)
// 8000-9FFF   8KB Video RAM (VRAM)      (switchable bank 0-1 in CGB Mode)
// A000-BFFF   8KB External RAM          (in cartridge, switchable bank, if any)
// C000-CFFF   4KB Work RAM Bank 0       (WRAM)
// D000-DFFF   4KB Work RAM Bank 1       (WRAM)  (switchable bank 1-7 in CGB Mode)
// E000-FDFF   Same as C000-DDFF (ECHO)  (typically not used)
// FE00-FE9F   Sprite Attribute Table    (OAM)
// FEA0-FEFF   Not Usable
// FF00-FF7F   I/O Ports
// FF80-FFFE   High RAM (HRAM)
// FFFF        Interrupt Enable Register
//
// See: http://bgb.bircd.org/pandocs.htm#cgbregisters
pub trait Memory {
    fn lb(&self, a: u16) -> u8;

    fn sb(&mut self, a: u16, v: u8);

    fn lh(&self, a: u16) -> u16 {
        u16::from(self.lb(a)) | (u16::from(self.lb(a.wrapping_add(1))) << 8)
    }

    fn sh(&mut self, a: u16, v: u16) {
        self.sb(a, v as u8);
        self.sb(a.wrapping_add(1), (v >> 8) as u8)
    }
}

// Stable is a trait for components that can save their state to disk, so that the game can be resumed later.
pub trait Stable: Memory {
    fn save(&self);
}

pub fn hi(n: u16) -> u8 {
    let n = n >> 8;
    n as u8
}

pub fn lo(n: u16) -> u8 {
    n as u8
}
