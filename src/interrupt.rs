use crate::convention::{Global, Memory};
use std::cell::RefCell;
use std::rc::Rc;

// FF0F   IF       Interrupt Flag (R/W)
// Bit 0: V-Blank  Interrupt Request (INT 40h)  (1=Request)
// Bit 1: LCD STAT Interrupt Request (INT 48h)  (1=Request)
// Bit 2: Timer    Interrupt Request (INT 50h)  (1=Request)
// Bit 3: Serial   Interrupt Request (INT 58h)  (1=Request)
// Bit 4: Joypad   Interrupt Request (INT 60h)  (1=Request)
#[rustfmt::skip]
pub enum InterruptFlag {
    VBlank = 0,
    LCD    = 1,
    Timer  = 2,
    Serial = 3,
    Joypad = 4,
}

pub struct Interrupt {
    glo: Rc<RefCell<Global>>,
}

impl Interrupt {
    pub fn power_up(glo: Rc<RefCell<Global>>) -> Self {
        // At the moment the Game Boy is powered on, the raw hardware initial value of IF is typically 0xE1.
        glo.borrow_mut().intf = 0xe1;
        glo.borrow_mut().inte = 0x00;
        Self { glo }
    }

    // Create an Interrupt instance with the given global reference, without modifying the initial interrupt flag and
    // enable values.
    pub fn owned(glo: Rc<RefCell<Global>>) -> Self {
        Self { glo }
    }

    // Set the specified interrupt flag to 1, indicating that the corresponding interrupt has been requested.
    pub fn raise(&mut self, flag: InterruptFlag) {
        self.glo.borrow_mut().intf |= 1 << flag as u8;
    }
}

impl Memory for Interrupt {
    fn lb(&self, a: u16) -> u8 {
        match a {
            0xff0f => self.glo.borrow().intf | 0xe0,
            0xffff => self.glo.borrow().inte,
            _ => unreachable!(),
        }
    }

    fn sb(&mut self, a: u16, v: u8) {
        match a {
            0xff0f => self.glo.borrow_mut().intf = v,
            0xffff => self.glo.borrow_mut().inte = v,
            _ => unreachable!(),
        }
    }
}
