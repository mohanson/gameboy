use crate::convention::{Global, Memory, Term};
use crate::interrupt::InterruptFlag;
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
    glo: Rc<RefCell<Global>>,
    // Before a transfer, it holds the next byte that will go out.
    // During a transfer, it has a blend of the outgoing and incoming bytes. Each cycle, the leftmost bit is shifted
    // out (and over the wire) and the incoming bit is shifted in from the other side:
    pub data: u8,
    // Bit 7 - Transfer Start Flag (0=No transfer is in progress or requested, 1=Transfer in progress, or requested)
    // Bit 1 - Clock Speed (0=Normal, 1=Fast) ** CGB Mode Only **
    // Bit 0 - Shift Clock (0=External Clock, 1=Internal Clock)
    pub ctrl: u8,
    // Bits remaining in the current transfer (0 = idle).
    bits: u8,
    // The byte being transmitted (captured when the transfer starts, before shifting corrupts data).
    pub tx_byte: u8,
}

impl Serial {
    pub fn power_up(glo: Rc<RefCell<Global>>) -> Self {
        Self {
            glo: glo.clone(),
            data: 0x00,
            ctrl: match glo.borrow().term {
                Term::DMG => 0x7e,
                Term::CGB => 0x7f,
            },
            bits: 0,
            tx_byte: 0,
        }
    }

    /// Advance the serial clock by `cycles` T-cycles.
    /// Each falling edge of bit 8 of sdiv (every 512 T-cycles) shifts one bit when a
    /// transfer is active.  After 8 bits the interrupt is raised and the transfer-start flag
    /// (SC bit 7) is cleared.
    /// NOTE: must be called AFTER timer.tick(cycles) has already advanced glo.sdiv.
    pub fn tick(&mut self, cycles: u32) {
        if self.bits == 0 {
            return;
        }
        // Timer has already incremented glo.sdiv by `cycles`; reconstruct the pre-tick value
        // so we can scan the same T-cycle range for bit-8 falling edges.
        let sdiv_end = self.glo.borrow().sdiv;
        let sdiv_start = sdiv_end.wrapping_sub(cycles as u16);
        let mut sdiv = sdiv_start;
        for _ in 0..cycles {
            let old_bit8 = (sdiv >> 8) & 1;
            sdiv = sdiv.wrapping_add(1);
            let new_bit8 = (sdiv >> 8) & 1;

            // Falling edge of bit 8 (1 → 0): one serial bit clock.
            if old_bit8 == 1 && new_bit8 == 0 {
                // Shift data left; shift in 1 (no connected device → all-high line).
                self.data = self.data.wrapping_shl(1) | 0x01;
                self.bits -= 1;
                if self.bits == 0 {
                    self.ctrl &= !0x80; // clear Transfer Start Flag
                    self.glo.borrow_mut().intf |= 1 << InterruptFlag::Serial as u8;
                    return;
                }
            }
        }
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
                // Start a transfer: internal clock (bit 0 = 1) and Transfer Start (bit 7 = 1).
                if v & 0x81 == 0x81 {
                    self.tx_byte = self.data;
                    self.bits = 8;
                }
            }
            _ => unreachable!(),
        };
    }
}
