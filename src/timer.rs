// Sometimes it's useful to have a timer that interrupts at regular intervals for routines that require periodic or
// percise updates. The timer in the GameBoy has a selectable frequency of 4096, 16384, 65536, or 262144 Hertz.
// This frequency increments the Timer Counter (TIMA). When it overflows, it generates an interrupt. It is then loaded
// with the contents of Timer Modulo (TMA).
//
// See: http://gbdev.gg8.se/wiki/articles/Timer_and_Divider_Registers
#[derive(Default)]
pub struct Timer {
    // Each time when the timer overflows (ie. when TIMA gets bigger than FFh), then an interrupt is requested by
    // setting Bit 2 in the IF Register (FF0F). When that interrupt is enabled, then the CPU will execute it by calling
    // the timer interrupt vector at 0050h.
    pub interrupt: u8,

    // This register is incremented at rate of 16384Hz (~16779Hz on SGB). Writing any value to this register resets it
    // to 00h.
    // Note: The divider is affected by CGB double speed mode, and will increment at 32768Hz in double speed.
    ff04: u8,
    // This timer is incremented by a clock frequency specified by the TAC register ($FF07). When the value overflows
    // (gets bigger than FFh) then it will be reset to the value specified in TMA (FF06), and an interrupt will be
    // requested, as described below.
    ff05: u8,
    // When the TIMA overflows, this data will be loaded.
    ff06: u8,
    //  Bit  2   - Timer Enable
    //  Bits 1-0 - Input Clock Select
    //             00: CPU Clock / 1024 (DMG, CGB:   4096 Hz, SGB:   ~4194 Hz)
    //             01: CPU Clock / 16   (DMG, CGB: 262144 Hz, SGB: ~268400 Hz)
    //             10: CPU Clock / 64   (DMG, CGB:  65536 Hz, SGB:  ~67110 Hz)
    //             11: CPU Clock / 256  (DMG, CGB:  16384 Hz, SGB:  ~16780 Hz)
    ff07: u8,

    freq: usize,
    tmp1: usize,
    tmp2: usize,
}

impl Timer {
    pub fn power_up() -> Self {
        Timer {
            freq: 256,
            ..Timer::default()
        }
    }

    pub fn get(&self, a: u16) -> u8 {
        match a {
            0xff04 => self.ff04,
            0xff05 => self.ff05,
            0xff06 => self.ff06,
            0xff07 => self.ff07,
            _ => panic!("Unsupported address"),
        }
    }

    pub fn set(&mut self, a: u16, v: u8) {
        match a {
            0xff04 => self.ff04 = 0x00,
            0xff05 => self.ff05 = v,
            0xff06 => self.ff06 = v,
            0xff07 => {
                self.ff07 = v;
                match v & 0x03 {
                    0x00 => self.freq = 1024,
                    0x01 => self.freq = 16,
                    0x02 => self.freq = 64,
                    0x03 => self.freq = 256,
                    _ => panic!(""),
                };
            }
            _ => panic!("Unsupported address"),
        }
    }

    pub fn next(&mut self, cycles: usize) {
        let c = 256;
        self.tmp1 += cycles;
        if self.tmp1 >= c {
            self.ff04 = self.ff04.wrapping_add(1);
            self.tmp1 -= c;
        }
        if (self.ff07 & 0x04) != 0x00 {
            self.tmp2 += cycles;
            if self.tmp2 >= self.freq {
                self.ff05 = self.ff05.wrapping_add(1);
                if self.ff05 == 0x00 {
                    self.ff05 = self.ff06;
                    self.interrupt |= 0x04;
                }
                self.tmp2 -= self.freq;
            }
        }
    }
}
