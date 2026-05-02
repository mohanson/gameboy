// Sometimes it's useful to have a timer that interrupts at regular intervals for routines that require periodic or
// percise updates. The timer in the GameBoy has a selectable frequency of 4096, 16384, 65536, or 262144 Hertz.
// This frequency increments the Timer Counter (TIMA). When it overflows, it generates an interrupt. It is then loaded
// with the contents of Timer Modulo (TMA).
//
// See: http://gbdev.gg8.se/wiki/articles/Timer_and_Divider_Registers
use super::convention::{Memory, Term};
use super::interrupt::{Interrupt, InterruptFlag};
use super::rng;
use std::cell::RefCell;
use std::ops::Shr;
use std::rc::Rc;

pub struct Timer {
    term: Term,
    intr: Rc<RefCell<Interrupt>>,
    // This register is incremented at rate of 16384Hz (~16779Hz on SGB). Writing any value to this register resets it
    // to 00h.
    sdiv: u16,
    // This timer is incremented by a clock frequency specified by the TAC register ($FF07). When the value overflows
    // (gets bigger than FFh) then it will be reset to the value specified in TMA (FF06), and an interrupt will be
    // requested, as described below.
    tima: u8,
    // When the TIMA overflows, this data will be loaded.
    tma: u8,
    //  Bit  2   - Timer Enable
    //  Bits 1-0 - Input Clock Select
    //             00: CPU Clock / 1024 (DMG, CGB:   4096 Hz, SGB:   ~4194 Hz)
    //             01: CPU Clock / 16   (DMG, CGB: 262144 Hz, SGB: ~268400 Hz)
    //             10: CPU Clock / 64   (DMG, CGB:  65536 Hz, SGB:  ~67110 Hz)
    //             11: CPU Clock / 256  (DMG, CGB:  16384 Hz, SGB:  ~16780 Hz)
    tac: u8,

    // Temporary variable for edge detection and delayed reload.
    signal: u8,
    // Combined overflow-recovery counter:
    //   0     = idle
    //   1..=4 = reload window  (TIMA writes ignored; TMA writes also update TIMA)
    //   5..=8 = delays window  (TIMA write cancels pending reload)
    delays: u8,
}

impl Timer {
    pub fn power_up(term: Term, intr: Rc<RefCell<Interrupt>>) -> Self {
        Timer {
            term,
            intr,
            sdiv: match term {
                Term::DMG => 0xabff,
                Term::CGB => rng::u16(),
            },
            tima: 0,
            tma: 0,
            tac: 0xf8,
            signal: 0,
            delays: 0,
        }
    }

    pub fn edge(&mut self) {
        let bitpos = [9, 3, 5, 7][self.tac as usize & 0x03];
        let bitval = ((self.sdiv >> bitpos) & 1) as u8;
        let enable = (self.tac & 0x04) >> 2;
        let signal = match self.term {
            Term::DMG => bitval & enable,
            Term::CGB => bitval,
        };
        let detect = if self.term == Term::DMG {
            self.signal == 1 && signal == 0
        } else {
            self.signal == 1 && bitval == 0 && enable == 1
        };
        if detect {
            let (addon, b) = self.tima.overflowing_add(1);
            self.tima = addon;
            if b {
                self.delays = 8;
            }
        }
        self.signal = signal;
    }

    pub fn tick(&mut self, cycles: u32) {
        for _ in 0..cycles {
            if self.delays > 0 {
                self.delays -= 1;
                if self.delays == 4 {
                    self.tima = self.tma;
                    self.intr.borrow_mut().raise(InterruptFlag::Timer);
                }
            }
            self.sdiv = self.sdiv.wrapping_add(1);
            self.edge();
        }
    }
}

impl Memory for Timer {
    fn lb(&self, a: u16) -> u8 {
        match a {
            0xff04 => self.sdiv.shr(8) as u8,
            0xff05 => self.tima,
            0xff06 => self.tma,
            0xff07 => 0xf8 | self.tac,
            _ => unreachable!(),
        }
    }

    fn sb(&mut self, a: u16, v: u8) {
        match a {
            0xff04 => {
                self.sdiv = 0;
                self.edge();
            }
            0xff05 => {
                // Writes during reload window are ignored by hardware.
                if self.delays > 0 && self.delays <= 4 {
                    return;
                }
                self.tima = v;
                self.delays = 0;
            }
            0xff06 => {
                self.tma = v;
                if self.delays > 0 && self.delays <= 4 {
                    self.tima = v;
                }
            }
            0xff07 => {
                self.tac = v;
                self.edge();
            }
            _ => unreachable!(),
        }
    }
}
