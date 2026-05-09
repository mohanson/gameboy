// Sometimes it's useful to have a timer that interrupts at regular intervals for routines that require periodic or
// percise updates. The timer in the GameBoy has a selectable frequency of 4096, 16384, 65536, or 262144 Hertz.
// This frequency increments the Timer Counter (TIMA). When it overflows, it generates an interrupt. It is then loaded
// with the contents of Timer Modulo (TMA).
//
// See: http://gbdev.gg8.se/wiki/articles/Timer_and_Divider_Registers
use crate::convention::{Global, Memory, Term, Ticker, hi};
use crate::interrupt::{Interrupt, InterruptFlag};
use crate::rng;
use std::cell::RefCell;
use std::rc::Rc;

pub struct Timer {
    glo: Rc<RefCell<Global>>,

    // This timer is incremented by a clock frequency specified by the TAC register ($FF07). When the value overflows
    // (gets bigger than FFh) then it will be reset to the value specified in TMA (FF06), and an interrupt will be
    // requested, as described below.
    ima: u8,
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
    window: u8,
}

impl Timer {
    pub fn power_up(glo: Rc<RefCell<Global>>) -> Self {
        let sdiv = match glo.borrow().term {
            Term::DMG => 0xabcc,
            Term::CGB => rng::u16(),
        };
        glo.borrow_mut().sdiv = sdiv;
        Timer { glo, ima: 0, tma: 0, tac: 0xf8, signal: 0, window: 0 }
    }

    pub fn edge(&mut self) {
        let bitpos = [9, 3, 5, 7][self.tac as usize & 0x03];
        let bitval = ((self.glo.borrow().sdiv >> bitpos) & 1) as u8;
        let enable = (self.tac & 0x04) >> 2;
        let signal = match self.glo.borrow().term {
            Term::DMG => bitval & enable,
            Term::CGB => bitval,
        };
        let detect = match self.glo.borrow().term {
            Term::DMG => self.signal == 1 && signal == 0,
            Term::CGB => self.signal == 1 && bitval == 0 && enable == 1,
        };
        if detect {
            self.ima = self.ima.wrapping_add(1);
            if self.ima == 0 {
                self.window = 8;
            }
        }
        self.signal = signal;
    }

    pub fn window_reload(&self) -> bool {
        self.window > 0 && self.window <= 4
    }

    pub fn window_delays(&self) -> bool {
        self.window > 4 && self.window <= 8
    }
}

impl Memory for Timer {
    fn lb(&self, a: u16) -> u8 {
        match a {
            0xff04 => hi(self.glo.borrow().sdiv),
            0xff05 => self.ima,
            0xff06 => self.tma,
            0xff07 => 0xf8 | self.tac,
            _ => unreachable!(),
        }
    }

    fn sb(&mut self, a: u16, v: u8) {
        match a {
            0xff04 => {
                self.glo.borrow_mut().sdiv = 0;
                self.edge();
            }
            0xff05 => {
                if self.window_delays() {
                    self.window = 0;
                }
                if self.window_reload() {
                    return;
                }
                self.ima = v;
            }
            0xff06 => {
                if self.window_reload() {
                    self.ima = v;
                }
                self.tma = v;
            }
            0xff07 => {
                self.tac = v;
                self.edge();
            }
            _ => unreachable!(),
        }
    }
}

impl Ticker for Timer {
    fn tick(&mut self, cycles: u32) {
        for _ in 0..cycles {
            if self.window != 0 {
                self.window = self.window.wrapping_sub(1);
            }
            if self.window == 4 {
                self.ima = self.tma;
                Interrupt::owned(self.glo.clone()).raise(InterruptFlag::Timer);
            }
            let sidv = self.glo.borrow().sdiv.wrapping_add(1);
            self.glo.borrow_mut().sdiv = sidv;
            if self.window == 0 {
                self.edge()
            }
        }
    }
}
