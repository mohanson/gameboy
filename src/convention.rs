use std::cell::RefCell;
use std::rc::Rc;

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

// Generic trait for memory access. It defines methods for reading and writing bytes and half-words (16 bits) at a
// given address.
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

// Hollow is a dummy memory that always reads 0xff and ignores writes. It can be used as a placeholder for components
// that are not yet implemented, or for testing purposes.
pub struct Hollow {}

impl Hollow {
    pub fn power_up() -> Self {
        Self {}
    }
}

impl Memory for Hollow {
    fn lb(&self, _: u16) -> u8 {
        0xff
    }

    fn sb(&mut self, _: u16, _: u8) {}
}

// Global is a struct that holds global state that may be needed by multiple components.
pub struct Global {
    // The interrupt enable register, which controls which interrupts are enabled.
    pub inte: u8,
    // The interrupt flag register, which indicates which interrupts are currently pending.
    pub intf: u8,
    // The serial division register, which is used for the serial communication feature of the gameboy.
    pub sdiv: u16,
    // Same as sdiv, but strictly increases every clock cycle.
    pub sdiw: u16,
    // The term of the gameboy.
    pub term: Term,
}

impl Global {
    pub fn power_up() -> Self {
        Self { inte: 0x00, intf: 0x00, sdiv: 0x0000, sdiw: 0x0000, term: Term::DMG }
    }

    pub fn share(self) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(self))
    }
}

// Stable is a trait for components that can save their state to disk, so that the game can be resumed later.
pub trait Stable: Memory {
    fn save(&self);
}

// Ticker is a trait for components that need to be updated every clock cycle.
pub trait Ticker: Memory {
    fn tick(&mut self, cycles: u32);
}

pub fn hi(n: u16) -> u8 {
    let n = n >> 8;
    n as u8
}

pub fn lo(n: u16) -> u8 {
    n as u8
}
