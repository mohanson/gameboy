use super::convention::Memory;

// FF0F   IF       Interrupt Flag (R/W)
// Bit 0: V-Blank  Interrupt Request (INT 40h)  (1=Request)
// Bit 1: LCD STAT Interrupt Request (INT 48h)  (1=Request)
// Bit 2: Timer    Interrupt Request (INT 50h)  (1=Request)
// Bit 3: Serial   Interrupt Request (INT 58h)  (1=Request)
// Bit 4: Joypad   Interrupt Request (INT 60h)  (1=Request)
#[rustfmt::skip]
#[derive(Clone)]
pub enum Flag {
    VBlank  = 0,
    LCDStat = 1,
    Timer   = 2,
    Serial  = 3,
    Joypad  = 4,
}

pub struct Intf {
    data: u8,
}

impl Intf {
    pub fn power_up() -> Self {
        // At the moment the Game Boy is powered on, the raw hardware initial value of IF is typically 0xE1.
        Self { data: 0xe1 }
    }

    // Set the specified interrupt flag to 1, indicating that the corresponding interrupt has been requested.
    pub fn raise(&mut self, flag: Flag) {
        self.data |= 1 << flag as u8;
    }
}

impl Memory for Intf {
    fn lb(&self, _: u16) -> u8 {
        self.data
    }

    fn sb(&mut self, _: u16, v: u8) {
        // Bits 5-7 are always 1, these are unused bits and default to high when read from the DMG.
        self.data = v | 0xe0;
    }
}
