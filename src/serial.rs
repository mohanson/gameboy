use crate::convention::{Memory, Term};
use crate::interrupt::{Interrupt, InterruptFlag};
use std::cell::RefCell;
use std::rc::Rc;

// Communication between two Gameboys happens one byte at a time. One Gameboy acts as the master, uses its internal
// clock, and thus controls when the exchange happens. The other one uses an external clock (i.e., the one inside the
// other Gameboy) and has no control over when the transfer happens. If it hasn't gotten around to loading up the next
// data byte at the time the transfer begins, the last one will go out again. Alternately, if it's ready to send the
// next byte but the last one hasn't gone out yet, it has no choice but to wait.
//
// See: http://gbdev.gg8.se/wiki/articles/Serial_Data_Transfer_(Link_Cable)
pub struct Serial {
    pub term: Term,
    // Before a transfer, it holds the next byte that will go out.
    // During a transfer, it has a blend of the outgoing and incoming bytes. Each cycle, the leftmost bit is shifted
    // out (and over the wire) and the incoming bit is shifted in from the other side:
    pub data: u8,
    // Bit 7 - Transfer Start Flag (0=No transfer is in progress or requested, 1=Transfer in progress, or requested)
    // Bit 1 - Clock Speed (0=Normal, 1=Fast) ** CGB Mode Only **
    // Bit 0 - Shift Clock (0=External Clock, 1=Internal Clock)
    pub ctrl: u8,
    intr: Rc<RefCell<Interrupt>>,
    // Internal clock divider, mirroring the timer's sdiv so that serial clock edges are
    // aligned to reset time (not to when SC is written).
    sdiv: u16,
    // Bits remaining in the current transfer (0 = idle).
    bits: u8,
}

impl Serial {
    pub fn power_up(term: Term, intr: Rc<RefCell<Interrupt>>) -> Self {
        Self {
            term,
            data: 0x00,
            ctrl: match term {
                Term::DMG => 0x7e,
                Term::CGB => 0x7f,
            },
            intr,
            // Same starting value as Timer::sdiv so clock edges are aligned.
            sdiv: match term {
                Term::DMG => 0xabcc,
                Term::CGB => 0x0000,
            },
            bits: 0,
        }
    }

    /// Called when FF04 (DIV) is written — reset the shared divider to keep in sync with Timer.
    pub fn reset_sdiv(&mut self) {
        self.sdiv = 0;
    }

    /// Advance the serial clock by `cycles` T-cycles.
    /// Each falling edge of bit 8 of sdiv (every 512 T-cycles) shifts one bit when a
    /// transfer is active.  After 8 bits the interrupt is raised and the transfer-start flag
    /// (SC bit 7) is cleared.
    pub fn tick(&mut self, cycles: u32) {
        for _ in 0..cycles {
            let old_bit8 = (self.sdiv >> 8) & 1;
            self.sdiv = self.sdiv.wrapping_add(1);
            let new_bit8 = (self.sdiv >> 8) & 1;

            // Falling edge of bit 8 (1 → 0): one serial bit clock.
            if old_bit8 == 1 && new_bit8 == 0 && self.bits > 0 {
                // Shift data left; shift in 1 (no connected device → all-high line).
                self.data = self.data.wrapping_shl(1) | 0x01;
                self.bits -= 1;
                if self.bits == 0 {
                    self.ctrl &= !0x80; // clear Transfer Start Flag
                    self.intr.borrow_mut().raise(InterruptFlag::Serial);
                }
            }
        }
    }
}

impl Memory for Serial {
    fn lb(&self, a: u16) -> u8 {
        match a {
            0xff01 => self.data,
            0xff02 => match self.term {
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
                // Start a transfer: internal clock (bit 0 = 1) and Transfer Start (bit 7 = 1).
                if v & 0x81 == 0x81 {
                    self.bits = 8;
                }
            }
            _ => unreachable!(),
        };
    }
}
