use super::convention::Memory;

// FF0F   IF       Interrupt Flag (R/W)
// Bit 0: V-Blank  Interrupt Request (INT 40h)  (1=Request)
// Bit 1: LCD STAT Interrupt Request (INT 48h)  (1=Request)
// Bit 2: Timer    Interrupt Request (INT 50h)  (1=Request)
// Bit 3: Serial   Interrupt Request (INT 58h)  (1=Request)
// Bit 4: Joypad   Interrupt Request (INT 60h)  (1=Request)
#[rustfmt::skip]
#[derive(Clone)]
pub enum InterruptFlag {
    VBlank  = 0,
    LCD     = 1,
    Timer   = 2,
    Serial  = 3,
    Joypad  = 4,
}

pub struct Interrupt {
    intf: u8,
    inte: u8,
}

impl Interrupt {
    pub fn power_up() -> Self {
        // At the moment the Game Boy is powered on, the raw hardware initial value of IF is typically 0xE1.
        Self { intf: 0xe1, inte: 0x00 }
    }

    // Set the specified interrupt flag to 1, indicating that the corresponding interrupt has been requested.
    pub fn raise(&mut self, flag: InterruptFlag) {
        self.intf |= 1 << flag as u8;
    }
}

impl Memory for Interrupt {
    fn lb(&self, a: u16) -> u8 {
        match a {
            0xff0f => self.intf | 0xe0,
            0xffff => self.inte,
            _ => unreachable!(),
        }
    }

    fn sb(&mut self, a: u16, v: u8) {
        match a {
            0xff0f => self.intf = v,
            0xffff => self.inte = v,
            _ => unreachable!(),
        }
    }
}
