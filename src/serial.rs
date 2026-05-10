// FF01 — SB: Serial transfer data
// FF02 — SC: Serial transfer control (bit7 = transfer enable, bit1 = CGB fast, bit0 = internal clock)
//
// The master uses its internal clock (8192 Hz normal, up to 512 kHz CGB) to shift one bit per
// falling edge of divider bit 8. After 8 bits, SC.7 is cleared and the Serial interrupt fires.
// A disconnected line pulls the input high, so an unconnected master receives $FF.
//
// https://gbdev.io/pandocs/Serial_Data_Transfer_(Link_Cable).html

use crate::convention::{Global, Memory, Term, Ticker};
use crate::interrupt::{Interrupt, InterruptFlag};
use std::cell::RefCell;
use std::rc::Rc;

pub struct Serial {
    glo: Rc<RefCell<Global>>,
    pub data: u8,    // SB: serial data (shifts MSB-first during transfer)
    pub ctrl: u8,    // SC: control
    bits: u8,        // bits remaining in the current transfer; 0 = idle
    pub tx_byte: u8, // byte latched at transfer start (SB is shifted in-place, original lost)
}

impl Serial {
    pub fn power_up(glo: Rc<RefCell<Global>>) -> Self {
        let ctrl = match glo.borrow().term {
            Term::DMG => 0x7e,
            Term::CGB => 0x7f,
        };
        Self { glo, data: 0, ctrl, bits: 0, tx_byte: 0 }
    }
}

impl Memory for Serial {
    fn lb(&self, a: u16) -> u8 {
        match a {
            0xff01 => self.data,
            0xff02 => match self.glo.borrow().term {
                Term::DMG => 0x7e | self.ctrl,
                Term::CGB => 0x7c | self.ctrl,
            },
            _ => unreachable!(),
        }
    }

    fn sb(&mut self, a: u16, v: u8) {
        match a {
            0xff01 => self.data = v,
            0xff02 => {
                self.ctrl = v;
                if v & 0x81 == 0x81 {
                    self.tx_byte = self.data;
                    self.bits = 8;
                }
            }
            _ => unreachable!(),
        }
    }
}

impl Ticker for Serial {
    // Called after timer.tick(cycles) has already advanced glo.sdiv.
    // A bit-8 falling edge of sdiv (every 512 T-cycles) clocks one serial bit.
    fn tick(&mut self, cycles: u32) {
        if self.bits == 0 {
            return;
        }
        let end = self.glo.borrow().sdiv;
        let mut sdiv = end.wrapping_sub(cycles as u16);
        for _ in 0..cycles {
            let prev = sdiv;
            sdiv = sdiv.wrapping_add(1);
            if (prev >> 8) & 1 == 1 && (sdiv >> 8) & 1 == 0 {
                self.data = (self.data << 1) | 1; // disconnected: input bit = 1
                self.bits -= 1;
                if self.bits == 0 {
                    self.ctrl &= !0x80;
                    Interrupt::owned(self.glo.clone()).raise(InterruptFlag::Serial);
                    return;
                }
            }
        }
    }
}
