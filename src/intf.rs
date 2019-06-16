// FF0F - IF - Interrupt Flag (R/W)
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
    pub data: u8,
}

impl Intf {
    pub fn power_up() -> Self {
        Self { data: 0x00 }
    }

    pub fn hi(&mut self, flag: Flag) {
        self.data |= 1 << flag as u8;
    }
}
