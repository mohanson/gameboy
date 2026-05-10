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
    data: u8,      // SB: serial data (shifts MSB-first during transfer)
    ctrl: u8,      // SC: control
    bits: u8,      // bits remaining in the current transfer; 0 = idle
    outs: Vec<u8>, // completed outgoing bytes, FIFO; capped at 256
}

impl Serial {
    pub fn power_up(glo: Rc<RefCell<Global>>) -> Self {
        let ctrl = match glo.borrow().term {
            Term::DMG => 0x7e,
            Term::CGB => 0x7f,
        };
        Self { glo, data: 0, ctrl, bits: 0, outs: Vec::new() }
    }

    // Returns the next byte received from the master, or None if the buffer is empty.
    pub fn read(&mut self) -> Option<u8> {
        if self.outs.is_empty() { None } else { Some(self.outs.remove(0)) }
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
                    if self.outs.len() == 256 {
                        self.outs.remove(0);
                    }
                    self.outs.push(self.data);
                    self.bits = 8;
                }
            }
            _ => unreachable!(),
        }
    }
}

impl Ticker for Serial {
    fn tick(&mut self, cycles: u32) {
        assert!(cycles <= u16::MAX as u32);
        if self.bits == 0 {
            return;
        }
        let mut sdiv = self.glo.borrow().sdiv.wrapping_sub(cycles as u16);
        for _ in 0..cycles {
            sdiv = sdiv.wrapping_add(1);
            if sdiv & 0x01ff == 0x0000 {
                self.data = (self.data << 1) | 1;
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
