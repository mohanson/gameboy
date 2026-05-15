use crate::convention::{CLOCK_FREQUENCY, Global, Memory, SAMPLE_RATE, Signal, Term, Ticker, hi, lo};
use blip_buf::BlipBuf;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

// Down-counting clock divider. Step advances by c ticks and returns the number of times the divider fired (= rolled
// over to zero).
struct Tmr {
    p: u32,
    n: u32,
}

impl Tmr {
    fn power_up(p: u32) -> Self {
        Self { p, n: 0 }
    }

    fn step(&mut self, c: u32) -> u32 {
        self.n += c;
        let r = self.n / self.p;
        self.n %= self.p;
        r
    }
}

// Frame Sequencer.
// The frame sequencer generates low frequency clocks for the modulation units. It is clocked by a 512 Hz timer.
//
// Step   Length Ctr  Vol Env     Sweep
// ---------------------------------------
// 0      Clock       -           -
// 1      -           -           -
// 2      Clock       -           Clock
// 3      -           -           -
// 4      Clock       -           -
// 5      -           -           -
// 6      Clock       -           Clock
// 7      -           Clock       -
// ---------------------------------------
// Rate   256 Hz      64 Hz       128 Hz
struct Fseq {
    s: u8,
}

#[rustfmt::skip]
impl Fseq {
    fn power_up() -> Self {
        Self { s: 0 }
    }

    fn step(&mut self) -> u8 {
        self.s = (self.s + 1) & 7;
        self.s
    }

    // Even steps fire the length counter.
    fn len(s: u8) -> bool { s & 1 == 0 }

    // Steps 2 and 6 fire the sweep.
    fn swp(s: u8) -> bool { s == 2 || s == 6 }

    // Step 7 fires the envelope.
    fn env(s: u8) -> bool { s == 7 }
}

// Buffered Blip channel: amplitude is encoded as deltas at integer-time positions, then resampled by blip_buf.
struct Blip {
    d: BlipBuf,
    t: u32, // Current time within the frame.
    a: i32, // Last amplitude.
}

impl Blip {
    fn power_up(sr: u32) -> Self {
        let mut d = BlipBuf::new(sr);
        d.set_rates(f64::from(CLOCK_FREQUENCY), f64::from(sr));
        Self { d, t: 0, a: 0 }
    }

    fn put(&mut self, t: u32, a: i32) {
        self.t = t;
        let dl = a - self.a;
        self.a = a;
        self.d.add_delta(t, dl);
    }

    fn end(&mut self, p: u32) {
        self.d.end_frame(p);
        self.t = self.t.wrapping_sub(p);
    }
}

// Name Addr 7654 3210 Function
// -----------------------------------------------------------------
//        Square 1
// NR10 FF10 -PPP NSSS Sweep period, negate, shift
// NR11 FF11 DDLL LLLL Duty, Length load (64-L)
// NR12 FF12 VVVV APPP Starting volume, Envelope add mode, period
// NR13 FF13 FFFF FFFF Frequency LSB
// NR14 FF14 TL-- -FFF Trigger, Length enable, Frequency MSB
//
//        Square 2
//      FF15 ---- ---- Not used
// NR21 FF16 DDLL LLLL Duty, Length load (64-L)
// NR22 FF17 VVVV APPP Starting volume, Envelope add mode, period
// NR23 FF18 FFFF FFFF Frequency LSB
// NR24 FF19 TL-- -FFF Trigger, Length enable, Frequency MSB
//
//        Wave
// NR30 FF1A E--- ---- DAC power
// NR31 FF1B LLLL LLLL Length load (256-L)
// NR32 FF1C -VV- ---- Volume code (00=0%, 01=100%, 10=50%, 11=25%)
// NR33 FF1D FFFF FFFF Frequency LSB
// NR34 FF1E TL-- -FFF Trigger, Length enable, Frequency MSB
//
//        Noise
//      FF1F ---- ---- Not used
// NR41 FF20 --LL LLLL Length load (64-L)
// NR42 FF21 VVVV APPP Starting volume, Envelope add mode, period
// NR43 FF22 SSSS WDDD Clock shift, Width mode of LFSR, Divisor code
// NR44 FF23 TL-- ---- Trigger, Length enable
//
//        Control/Status
// NR50 FF24 ALLL BRRR Vin L enable, Left vol, Vin R enable, Right vol
// NR51 FF25 NW21 NW21 Left enables, Right enables
// NR52 FF26 P--- NW21 Power control/status, Channel length statuses
//
//        Not used
//      FF27 ---- ----
//      .... ---- ----
//      FF2F ---- ----
//
//        Wave Table
//      FF30 0000 1111 Samples 0 and 1
//      ....
//      FF3F 0000 1111 Samples 30 and 31
struct Reg {
    n: [u8; 5],
}

#[rustfmt::skip]
impl Reg {
    fn power_up() -> Self { Self { n: [0; 5] } }
    fn bit(&self, i: usize, b: u8) -> bool { self.n[i] & (1 << b) != 0 }

    // Common.
    fn len_en(&self) -> bool { self.bit(4, 6) }
    fn freq(&self) -> u16 { (u16::from(self.n[4] & 0x07) << 8) | u16::from(self.n[3]) }
    fn freq_update(&mut self, f: u16) {
        self.n[3] = lo(f);
        self.n[4] = (self.n[4] & 0xf8) | (hi(f) & 0x07);
    }

    // CH1 / CH2 / CH4 (envelope channels).
    fn dac_sq(&self)  -> bool { self.n[2] & 0xf8 != 0 }
    fn duty(&self)    -> u8   { self.n[1] >> 6 }
    fn env_add(&self) -> bool { self.bit(2, 3) }
    fn env_p(&self)   -> u8   { self.n[2] & 0x07 }
    fn vol_init(&self)-> u8   { self.n[2] >> 4 }
    fn len_load_sq(&self) -> u16 { 64 - u16::from(self.n[1] & 0x3f) }
    fn per_sq(&self)  -> u32  { 4 * (2048 - u32::from(self.freq())) }

    // CH1 sweep.
    fn swp_neg(&self) -> bool { self.bit(0, 3) }
    fn swp_p(&self)   -> u8   { (self.n[0] >> 4) & 0x07 }
    fn swp_sh(&self)  -> u8   { self.n[0] & 0x07 }

    // CH3 wave.
    fn dac_wv(&self)  -> bool { self.bit(0, 7) }
    fn len_load_wv(&self) -> u16 { 256 - u16::from(self.n[1]) }
    fn per_wv(&self)  -> u32  { 2 * (2048 - u32::from(self.freq())) }
    fn vol_code(&self)-> u8   { (self.n[2] >> 5) & 0x03 }

    // CH4 noise.
    fn div_code(&self)-> u8   { self.n[3] & 0x07 }
    fn shift_ns(&self)-> u8   { self.n[3] >> 4 }
    fn width7(&self)  -> bool { self.bit(3, 3) }
    fn per_ns(&self)  -> u32  {
        let d: u32 = match self.div_code() {
            0 => 8,
            n => u32::from(n) * 16,
        };
        d << self.shift_ns()
    }
}

// Length counter. Returns true from step if the channel must be disabled (decrement to zero with length-enable set).
struct Lc {
    n: u16,
}

impl Lc {
    fn power_up() -> Self {
        Self { n: 0 }
    }

    fn step(&mut self, reg: &Reg) -> bool {
        if reg.len_en() && self.n != 0 {
            self.n -= 1;
            return self.n == 0;
        }
        false
    }
}

// Volume envelope (CH1/CH2/CH4).
struct Env {
    t: Tmr,
    v: u8,
}

impl Env {
    fn power_up() -> Self {
        Self { t: Tmr::power_up(8), v: 0 }
    }

    // Reload on trigger. Period 0 is treated as 8.
    fn reload(&mut self, reg: &Reg) {
        self.t.p = if reg.env_p() == 0 { 8 } else { u32::from(reg.env_p()) };
        self.t.n = 0;
        self.v = reg.vol_init();
    }

    fn step(&mut self, reg: &Reg) {
        if reg.env_p() == 0 || self.t.step(1) == 0 {
            return;
        }
        let n = if reg.env_add() { self.v.wrapping_add(1) } else { self.v.wrapping_sub(1) };
        if n <= 15 {
            self.v = n;
        }
    }
}

// CH1 frequency sweep.
struct Swp {
    t: Tmr,
    e: bool,
    s: u16,  // Shadow frequency.
    n: bool, // Obscure: any negate-mode calc since trigger?
}

impl Swp {
    fn power_up() -> Self {
        Self { t: Tmr::power_up(8), e: false, s: 0, n: false }
    }

    fn calc(&mut self, r: &Reg) -> u16 {
        let d = self.s >> r.swp_sh();
        if r.swp_neg() {
            self.n = true;
            self.s.wrapping_sub(d)
        } else {
            self.s.wrapping_add(d)
        }
    }

    // Pan Docs "negate->positive" obscure rule: clearing the negate bit after at least one negate calculation since
    // trigger disables CH1.
    fn nr10di(&self, r: &Reg, v: u8) -> bool {
        let old = r.swp_neg();
        let new = v & 0x08 != 0;
        old && !new && self.n
    }

    // Reload on trigger. Returns true iff the initial overflow check disables the channel.
    fn reload(&mut self, r: &Reg) -> bool {
        self.s = r.freq();
        let p = r.swp_p();
        self.t.p = if p == 0 { 8 } else { u32::from(p) };
        self.t.n = 0;
        self.e = p != 0 || r.swp_sh() != 0;
        self.n = false;
        if r.swp_sh() != 0 {
            return self.calc(r) >= 2048;
        }
        false
    }

    fn step(&mut self, r: &mut Reg) -> bool {
        let fired = self.t.step(1) != 0;
        if fired {
            // Reload period from the current register on every fire so writes to NR10 between fires take effect
            // immediately. Period 0 -> 8.
            let p = r.swp_p();
            self.t.p = if p == 0 { 8 } else { u32::from(p) };
            self.t.n = 0;
        }
        if !fired || !self.e || r.swp_p() == 0 {
            return false;
        }
        let f = self.calc(r);
        if f >= 2048 {
            return true;
        }
        if r.swp_sh() != 0 {
            self.s = f;
            r.freq_update(f);
            if self.calc(r) >= 2048 {
                return true;
            }
        }
        false
    }
}

// 15-bit LFSR with optional 7-bit feedback path for CH4.
struct Lfsr {
    n: u16,
}

impl Lfsr {
    fn power_up() -> Self {
        Self { n: 0x7fff }
    }

    fn reload(&mut self) {
        self.n = 0x7fff;
    }

    // Returns true if the new low bit is 1 (the channel outputs ~bit0).
    fn step(&mut self, reg: &Reg) -> bool {
        let b = ((self.n ^ (self.n >> 1)) & 1) as u16;
        self.n = (self.n >> 1) | (b << 14);
        if reg.width7() {
            self.n = (self.n & !(1 << 6)) | (b << 6);
        }
        self.n & 1 == 0
    }
}

const DUTY: [u8; 4] = [0b0000_0001, 0b1000_0001, 0b1000_0111, 0b0111_1110];

// CH1: square wave with sweep.
struct Sq1 {
    buf: Blip,
    idx: u8,
    reg: Reg,
    tmr: Tmr,
    fs: Swp,
    lc: Lc,
    on: bool,
    ve: Env,
}

impl Sq1 {
    fn power_up() -> Self {
        let mut reg = Reg::power_up();
        reg.n[1] = 0x40;
        Self {
            buf: Blip::power_up(SAMPLE_RATE),
            idx: 1,
            reg,
            tmr: Tmr::power_up(8192),
            fs: Swp::power_up(),
            lc: Lc::power_up(),
            on: false,
            ve: Env::power_up(),
        }
    }

    fn nrx4sb(&mut self, v: u8, xtra: bool) {
        let old_en = self.reg.len_en();
        self.reg.n[4] = v;
        self.tmr.p = self.reg.per_sq();
        if xtra && !old_en && self.lc.step(&self.reg) {
            self.on = false;
        }
        if v & 0x80 != 0 {
            let len0 = self.lc.n == 0;
            self.on = true;
            if self.lc.n == 0 {
                self.lc.n = 64;
            }
            self.tmr.p = self.reg.per_sq();
            self.ve.reload(&self.reg);
            if self.fs.reload(&self.reg) {
                self.on = false;
            }
            // Extra-length-clock obscure: if length was 0 and the FS just clocked length on the previous step, the
            // reloaded counter is decremented.
            if xtra && self.reg.len_en() && len0 && self.lc.n != 0 {
                self.lc.n -= 1;
            }
            if !self.reg.dac_sq() {
                self.on = false;
            }
        }
    }

    fn step(&mut self, c: u32) {
        let pat = DUTY[self.reg.duty() as usize];
        let v = i32::from(self.ve.v);
        for _ in 0..self.tmr.step(c) {
            let a = if !self.on || self.ve.v == 0 {
                0
            } else if (pat >> self.idx) & 1 != 0 {
                v
            } else {
                v * -1
            };
            self.buf.put(self.buf.t.wrapping_add(self.tmr.p), a);
            self.idx = (self.idx + 1) & 7;
        }
    }
}

impl Memory for Sq1 {
    fn lb(&self, a: u16) -> u8 {
        self.reg.n[(a - 0xff10) as usize]
    }

    fn sb(&mut self, a: u16, v: u8) {
        match a {
            0xff10 => self.reg.n[0] = v | 0x80,
            0xff11 => {
                self.reg.n[1] = v;
                self.lc.n = self.reg.len_load_sq();
            }
            0xff12 => {
                self.reg.n[2] = v;
                if !self.reg.dac_sq() {
                    self.on = false;
                }
            }
            0xff13 => {
                self.reg.n[3] = v;
                self.tmr.p = self.reg.per_sq();
            }
            0xff14 => self.nrx4sb(v, false),
            _ => unreachable!(),
        }
    }
}

// CH2: square wave, no sweep. Shares almost everything with CH1.
struct Sq2 {
    buf: Blip,
    idx: u8,
    reg: Reg,
    tmr: Tmr,
    on: bool,
    lc: Lc,
    ve: Env,
}

impl Sq2 {
    fn power_up() -> Self {
        let mut reg = Reg::power_up();
        reg.n[1] = 0x40;
        Self {
            buf: Blip::power_up(SAMPLE_RATE),
            idx: 1,
            reg,
            tmr: Tmr::power_up(8192),
            on: false,
            lc: Lc::power_up(),
            ve: Env::power_up(),
        }
    }

    fn nrx4sb(&mut self, v: u8, xtra: bool) {
        let old_en = self.reg.len_en();
        self.reg.n[4] = v;
        self.tmr.p = self.reg.per_sq();
        if xtra && !old_en && self.lc.step(&self.reg) {
            self.on = false;
        }
        if v & 0x80 != 0 {
            let len0 = self.lc.n == 0;
            self.on = true;
            if self.lc.n == 0 {
                self.lc.n = 64;
            }
            self.tmr.p = self.reg.per_sq();
            self.ve.reload(&self.reg);
            if xtra && self.reg.len_en() && len0 && self.lc.n != 0 {
                self.lc.n -= 1;
            }
            if !self.reg.dac_sq() {
                self.on = false;
            }
        }
    }

    fn step(&mut self, c: u32) {
        let pat = DUTY[self.reg.duty() as usize];
        let v = i32::from(self.ve.v);
        for _ in 0..self.tmr.step(c) {
            let a = if !self.on || self.ve.v == 0 {
                0
            } else if (pat >> self.idx) & 1 != 0 {
                v
            } else {
                v * -1
            };
            self.buf.put(self.buf.t.wrapping_add(self.tmr.p), a);
            self.idx = (self.idx + 1) & 7;
        }
    }
}

impl Memory for Sq2 {
    fn lb(&self, a: u16) -> u8 {
        self.reg.n[(a - 0xff15) as usize]
    }

    fn sb(&mut self, a: u16, v: u8) {
        match a {
            0xff15 => self.reg.n[0] = v,
            0xff16 => {
                self.reg.n[1] = v;
                self.lc.n = self.reg.len_load_sq();
            }
            0xff17 => {
                self.reg.n[2] = v;
                if !self.reg.dac_sq() {
                    self.on = false;
                }
            }
            0xff18 => {
                self.reg.n[3] = v;
                self.tmr.p = self.reg.per_sq();
            }
            0xff19 => self.nrx4sb(v, false),
            _ => unreachable!(),
        }
    }
}

struct Wv {
    buf: Blip,
    idx: usize,
    ram: [u8; 16],
    reg: Reg,
    tmr: Tmr,
    // 2-MHz cycles from first trigger to last NR33 write.
    d1: u32,
    lc: Lc,
    on: bool,
    // period at first trigger, in T-cycles.
    p1: u32,
    // Live-position tracking for wave-RAM access while CH3 is active.
    t_apu_n: u32,
    t_frame: u32,
    // sdiv at last first-trigger (was inactive)
    t_s_div: u16,
}

impl Wv {
    fn power_up() -> Self {
        Self {
            buf: Blip::power_up(SAMPLE_RATE),
            idx: 0,
            ram: [0; 16],
            reg: Reg::power_up(),
            tmr: Tmr::power_up(8192),
            d1: 0,
            lc: Lc::power_up(),
            on: false,
            p1: 0,
            t_apu_n: 0,
            t_frame: 0,
            t_s_div: 0,
        }
    }

    fn nrx4sb(&mut self, v: u8, xtra: bool, dmg_off: Option<usize>) {
        let old_en = self.reg.len_en();
        self.reg.n[4] = v;
        self.tmr.p = self.reg.per_wv();
        if xtra && !old_en && self.lc.step(&self.reg) {
            self.on = false;
        }
        if v & 0x80 != 0 {
            // DMG retrigger-while-active wave-RAM corruption.
            if let Some(off) = dmg_off {
                if off < 4 {
                    self.ram[0] = self.ram[off];
                } else {
                    let a = off & !3;
                    self.ram.copy_within(a..a + 4, 0);
                }
            }
            let len0 = self.lc.n == 0;
            self.on = true;
            if self.lc.n == 0 {
                self.lc.n = 256;
            }
            self.tmr.p = self.reg.per_wv();
            self.tmr.n = 0;
            self.idx = 1;
            if xtra && self.reg.len_en() && len0 && self.lc.n != 0 {
                self.lc.n -= 1;
            }
            if !self.reg.dac_wv() {
                self.on = false;
            }
        }
    }

    fn step(&mut self, c: u32) {
        let s = match self.reg.vol_code() {
            0 => 4,
            1 => 0,
            2 => 1,
            _ => 2,
        };
        for _ in 0..self.tmr.step(c) {
            let nib = if self.idx & 1 == 0 { self.ram[self.idx / 2] >> 4 } else { self.ram[self.idx / 2] & 0x0f };
            let a = if self.on && self.reg.dac_wv() { i32::from(nib >> s) } else { 0 };
            self.buf.put(self.buf.t.wrapping_add(self.tmr.p), a);
            self.idx = (self.idx + 1) & 0x1f;
        }
    }
}

impl Memory for Wv {
    fn lb(&self, a: u16) -> u8 {
        match a {
            0xff1a..=0xff1e => self.reg.n[(a - 0xff1a) as usize],
            0xff30..=0xff3f => self.ram[(a - 0xff30) as usize],
            _ => unreachable!(),
        }
    }

    fn sb(&mut self, a: u16, v: u8) {
        match a {
            0xff1a => {
                self.reg.n[0] = v;
                if !self.reg.dac_wv() {
                    self.on = false;
                }
            }
            0xff1b => {
                self.reg.n[1] = v;
                self.lc.n = self.reg.len_load_wv();
            }
            0xff1c => self.reg.n[2] = v,
            0xff1d => {
                self.reg.n[3] = v;
                self.tmr.p = self.reg.per_wv();
            }
            0xff1e => self.nrx4sb(v, false, None),
            0xff30..=0xff3f => self.ram[(a - 0xff30) as usize] = v,
            _ => unreachable!(),
        }
    }
}

struct Ns {
    buf: Blip,
    reg: Reg,
    tmr: Tmr,
    lc: Lc,
    lf: Lfsr,
    on: bool,
    ve: Env,
}

impl Ns {
    fn power_up() -> Self {
        Self {
            buf: Blip::power_up(SAMPLE_RATE),
            reg: Reg::power_up(),
            tmr: Tmr::power_up(8),
            lc: Lc::power_up(),
            lf: Lfsr::power_up(),
            on: false,
            ve: Env::power_up(),
        }
    }

    fn nrx4sb(&mut self, v: u8, xtra: bool) {
        let old_en = self.reg.len_en();
        self.reg.n[4] = v;
        if xtra && !old_en && self.lc.step(&self.reg) {
            self.on = false;
        }
        if v & 0x80 != 0 {
            let len0 = self.lc.n == 0;
            self.on = true;
            if self.lc.n == 0 {
                self.lc.n = 64;
            }
            self.tmr.p = self.reg.per_ns();
            self.ve.reload(&self.reg);
            self.lf.reload();
            if xtra && self.reg.len_en() && len0 && self.lc.n != 0 {
                self.lc.n -= 1;
            }
            if !self.reg.dac_sq() {
                self.on = false;
            }
        }
    }

    fn step(&mut self, c: u32) {
        let v = i32::from(self.ve.v);
        for _ in 0..self.tmr.step(c) {
            let hi = self.lf.step(&self.reg);
            let a = if !self.on || self.ve.v == 0 {
                0
            } else if hi {
                v
            } else {
                v * -1
            };
            self.buf.put(self.buf.t.wrapping_add(self.tmr.p), a);
        }
    }
}

impl Memory for Ns {
    fn lb(&self, a: u16) -> u8 {
        self.reg.n[(a - 0xff1f) as usize]
    }

    fn sb(&mut self, a: u16, v: u8) {
        match a {
            0xff1f => self.reg.n[0] = v,
            0xff20 => {
                self.reg.n[1] = v;
                self.lc.n = self.reg.len_load_sq();
            }
            0xff21 => {
                self.reg.n[2] = v;
                if !self.reg.dac_sq() {
                    self.on = false;
                }
            }
            0xff22 => {
                self.reg.n[3] = v;
                self.tmr.p = self.reg.per_ns();
            }
            0xff23 => self.nrx4sb(v, false),
            _ => unreachable!(),
        }
    }
}

// Or-mask applied when reading FF10..FF3F (unused/read-only bits read as 1).
const RD: [u8; 0x30] = [
    0x80, 0x3f, 0x00, 0xff, 0xbf, 0xff, 0x3f, 0x00, 0xff, 0xbf, // FF10..FF19
    0x7f, 0xff, 0x9f, 0xff, 0xbf, 0xff, 0xff, 0x00, 0x00, 0xbf, // FF1A..FF23
    0x00, 0x00, 0x70, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, // FF24..FF2D
    0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // FF2E..FF37
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // FF38..FF3F
];

pub struct Apu {
    glo: Rc<RefCell<Global>>,

    pub data: Arc<Mutex<Vec<(f32, f32)>>>,
    pub sdiv: u16,

    ch1: Sq1,
    ch2: Sq2,
    ch3: Wv,
    ch4: Ns,
    fc: u32,
    fs: Fseq,
    fs_skip: Signal,
    ft: Tmr,
    nr50: u8,
    nr51: u8,
    pwr: bool,
    sr: u32,
}

impl Apu {
    pub fn power_up(glo: Rc<RefCell<Global>>, sr: u32) -> Self {
        Self {
            glo,
            data: Arc::new(Mutex::new(Vec::new())),
            sdiv: 0,
            ch1: Sq1::power_up(),
            ch2: Sq2::power_up(),
            ch3: Wv::power_up(),
            ch4: Ns::power_up(),
            fc: 0,
            fs: Fseq::power_up(),
            fs_skip: Signal::power_up(),
            ft: Tmr::power_up(CLOCK_FREQUENCY / 512),
            nr50: 0,
            nr51: 0,
            pwr: false,
            sr,
        }
    }

    fn add(&self, l: &[f32], r: &[f32]) {
        let mut b = self.data.lock().unwrap();
        for (a, c) in l.iter().zip(r) {
            if b.len() > self.sr as usize {
                return;
            }
            b.push((*a, *c));
        }
    }

    fn mix(&mut self) {
        let n = self.ch1.buf.d.samples_avail() as usize;
        debug_assert_eq!(n, self.ch2.buf.d.samples_avail() as usize);
        debug_assert_eq!(n, self.ch3.buf.d.samples_avail() as usize);
        debug_assert_eq!(n, self.ch4.buf.d.samples_avail() as usize);

        let vl = self.vol_l();
        let vr = self.vol_r();
        let pan = self.nr51;
        let mut done = 0usize;
        while done < n {
            let mut l = [0f32; 2048];
            let mut r = [0f32; 2048];
            let mut t = [0i16; 2048];

            let read = |b: &mut Blip, t: &mut [i16]| b.d.read_samples(t, false);
            let mix_into = |t: &[i16], l: &mut [f32], r: &mut [f32], lp: bool, rp: bool| {
                for (i, &s) in t.iter().enumerate() {
                    let f = f32::from(s);
                    if lp {
                        l[i] += f * vl;
                    }
                    if rp {
                        r[i] += f * vr;
                    }
                }
            };

            let c1 = read(&mut self.ch1.buf, &mut t);
            mix_into(&t[..c1], &mut l, &mut r, pan & 0x10 != 0, pan & 0x01 != 0);
            let c2 = read(&mut self.ch2.buf, &mut t);
            mix_into(&t[..c2], &mut l, &mut r, pan & 0x20 != 0, pan & 0x02 != 0);
            let c3 = read(&mut self.ch3.buf, &mut t);
            mix_into(&t[..c3], &mut l, &mut r, pan & 0x40 != 0, pan & 0x04 != 0);
            let c4 = read(&mut self.ch4.buf, &mut t);
            mix_into(&t[..c4], &mut l, &mut r, pan & 0x80 != 0, pan & 0x08 != 0);

            debug_assert!(c1 == c2 && c2 == c3 && c3 == c4);
            self.add(&l[..c1], &r[..c1]);
            done += c1;
        }
    }

    fn vol_l(&self) -> f32 {
        f32::from((self.nr50 >> 4) & 0x07) / 7.0 / 15.0 * 0.25
    }

    fn vol_r(&self) -> f32 {
        f32::from(self.nr50 & 0x07) / 7.0 / 15.0 * 0.25
    }

    // Compute the live wave sample index based on elapsed T-cycles since the last trigger. The wave timer fires once
    // every period T-cycles after an initial period_at_trigger + 6 T-cycle warm-up.
    fn live_wave(&self) -> u64 {
        let p_now = u64::from(self.ch3.tmr.p);
        let p_trg = u64::from(self.ch3.p1);
        let d = u64::from(self.fc.wrapping_sub(self.ch3.t_frame));
        let elapsed = d * u64::from(self.ft.p) + u64::from(self.ft.n) - u64::from(self.ch3.t_apu_n);
        let t_first = p_trg + 6;
        if elapsed < t_first { 0 } else { 1 + (elapsed - t_first) / p_now }
    }

    fn wave_addr(&self, fires: u64) -> usize {
        ((fires & 0x1f) as usize) / 2
    }

    fn dmg_wave_window(&self, fires: u64) -> bool {
        if fires == 0 {
            return false;
        }
        let p = u64::from(self.ch3.tmr.p);
        let t_first = u64::from(self.ch3.p1) + 6;
        let d = u64::from(self.fc.wrapping_sub(self.ch3.t_frame));
        let elapsed = d * u64::from(self.ft.p) + u64::from(self.ft.n) - u64::from(self.ch3.t_apu_n);
        let t_last = t_first + (fires - 1) * p;
        elapsed - t_last < 2
    }

    fn power_no(&mut self) {
        // Zero every channel register except wave RAM and (on DMG) length counters.
        for i in 0..5 {
            self.ch1.reg.n[i] = 0;
        }
        for i in 0..5 {
            self.ch2.reg.n[i] = 0;
        }
        for i in 0..5 {
            self.ch3.reg.n[i] = 0;
        }
        for i in 0..5 {
            self.ch4.reg.n[i] = 0;
        }
        self.ch1.on = false;
        self.ch2.on = false;
        self.ch3.on = false;
        self.ch4.on = false;
        self.nr50 = 0;
        self.nr51 = 0;
        if self.glo.borrow().term == Term::CGB {
            self.ch1.lc.n = 0;
            self.ch2.lc.n = 0;
            self.ch3.lc.n = 0;
            self.ch4.lc.n = 0;
        }
    }

    fn power_on(&mut self) {
        // Phase-align the frame sequencer with DIV: the next FS clock occurs at the next falling edge of sdiv bit 12.
        // If that bit is already high at power-on, the very first FS event is skipped (hardware glitch).
        self.fs.s = 7;
        self.ft.n = u32::from(self.sdiv) % (CLOCK_FREQUENCY / 512);
        if self.sdiv & 0x1000 != 0 {
            self.fs_skip.set();
        }
    }

    fn status(&self) -> u8 {
        (if self.pwr { 0x80 } else { 0 })
            | (if self.ch4.on { 8 } else { 0 })
            | (if self.ch3.on && self.ch3.reg.dac_wv() { 4 } else { 0 })
            | (if self.ch2.on { 2 } else { 0 })
            | (if self.ch1.on { 1 } else { 0 })
    }
}

impl Memory for Apu {
    fn lb(&self, a: u16) -> u8 {
        let v = match a {
            0xff10..=0xff14 => self.ch1.lb(a),
            0xff15..=0xff19 => self.ch2.lb(a),
            0xff1a..=0xff1e => self.ch3.lb(a),
            0xff1f..=0xff23 => self.ch4.lb(a),
            0xff24 => self.nr50,
            0xff25 => self.nr51,
            0xff26 => self.status(),
            0xff27..=0xff2f => 0,
            0xff30..=0xff3f => {
                if self.ch3.on && self.ch3.reg.dac_wv() {
                    let fires = self.live_wave();
                    let addr = self.wave_addr(fires);
                    match self.glo.borrow().term {
                        Term::CGB => self.ch3.ram[addr],
                        Term::DMG => {
                            if self.dmg_wave_window(fires) {
                                self.ch3.ram[addr]
                            } else {
                                0xff
                            }
                        }
                    }
                } else {
                    self.ch3.lb(a)
                }
            }
            _ => unreachable!(),
        };
        v | RD[(a - 0xff10) as usize]
    }

    fn sb(&mut self, a: u16, v: u8) {
        // Power gating: all channel/NR50/NR51 writes are blocked while the APU is off, except wave RAM and NR52
        // itself. On DMG, length-counter pre-loads via NRx1 remain writable.
        if !self.pwr && a != 0xff26 && !(0xff30..=0xff3f).contains(&a) {
            if self.glo.borrow().term == Term::DMG {
                match a {
                    0xff11 => {
                        self.ch1.reg.n[1] = v & 0x3f;
                        self.ch1.lc.n = self.ch1.reg.len_load_sq();
                    }
                    0xff16 => {
                        self.ch2.reg.n[1] = v & 0x3f;
                        self.ch2.lc.n = self.ch2.reg.len_load_sq();
                    }
                    0xff1b => {
                        self.ch3.reg.n[1] = v;
                        self.ch3.lc.n = self.ch3.reg.len_load_wv();
                    }
                    0xff20 => {
                        self.ch4.reg.n[1] = v;
                        self.ch4.lc.n = self.ch4.reg.len_load_sq();
                    }
                    _ => {}
                }
            }
            return;
        }

        // First half of the length period (Pan Docs obscure behaviour): the FS step just past an even step. step%2==0
        // means the next clock will be a length clock, i.e. we are in the half that triggers extras.
        let xtra = self.fs.s & 1 == 0;
        match a {
            0xff10 => {
                if self.ch1.fs.nr10di(&self.ch1.reg, v) {
                    self.ch1.on = false;
                }
                self.ch1.reg.n[0] = v | 0x80;
            }
            0xff11..=0xff13 => self.ch1.sb(a, v),
            0xff14 => self.ch1.nrx4sb(v, xtra),

            0xff15..=0xff18 => self.ch2.sb(a, v),
            0xff19 => self.ch2.nrx4sb(v, xtra),

            0xff1a..=0xff1c => self.ch3.sb(a, v),
            0xff1d => {
                // Record d1 (2-MHz cycles from first trigger to this NR33 write).
                if self.glo.borrow().term == Term::DMG && self.ch3.on && self.ch3.reg.dac_wv() {
                    let dt = self.sdiv.wrapping_sub(self.ch3.t_s_div) as u32;
                    self.ch3.d1 = dt / 2;
                }
                self.ch3.sb(a, v);
            }
            0xff1e => self.nr34sb(v, xtra),
            0xff1f..=0xff22 => self.ch4.sb(a, v),
            0xff23 => self.ch4.nrx4sb(v, xtra),
            0xff24 => self.nr50 = v,
            0xff25 => self.nr51 = v,
            0xff26 => {
                let was = self.pwr;
                self.pwr = v & 0x80 != 0;
                if was && !self.pwr {
                    self.power_no();
                } else if !was && self.pwr {
                    self.power_on();
                }
            }
            0xff27..=0xff2f => {}
            0xff30..=0xff3f => {
                if self.ch3.on && self.ch3.reg.dac_wv() {
                    let fires = self.live_wave();
                    let addr = self.wave_addr(fires);
                    match self.glo.borrow().term {
                        Term::CGB => self.ch3.ram[addr] = v,
                        Term::DMG => {
                            if self.dmg_wave_window(fires) {
                                self.ch3.ram[addr] = v;
                            }
                        }
                    }
                } else {
                    self.ch3.ram[(a - 0xff30) as usize] = v;
                }
            }
            _ => unreachable!(),
        }
    }
}

impl Apu {
    fn nr34sb(&mut self, v: u8, xtra: bool) {
        let was = self.ch3.on && self.ch3.reg.dac_wv();
        let sdiv = self.sdiv;
        let trig = v & 0x80 != 0;

        let dmg_off = if trig && was && self.glo.borrow().term == Term::DMG {
            let dt = sdiv.wrapping_sub(self.ch3.t_s_div) as u32 / 2;
            let d2 = dt.saturating_sub(self.ch3.d1);
            dmg_corrupt_offset(self.ch3.p1 / 2, self.ch3.d1, d2, self.ch3.tmr.p / 2)
        } else {
            None
        };

        if trig {
            self.ch3.t_s_div = sdiv;
            self.ch3.d1 = 0;
            self.ch3.t_apu_n = self.ft.n;
            self.ch3.t_frame = self.fc;
        }
        self.ch3.nrx4sb(v, xtra, dmg_off);
        if trig {
            self.ch3.p1 = self.ch3.tmr.p;
        }
    }
}

impl Ticker for Apu {
    fn tick(&mut self, cycles: u16) {
        // The DIV-APU 512 Hz counter runs regardless of power state.
        let n = self.ft.step(u32::from(cycles));
        if !self.pwr {
            for _ in 0..n {
                if self.fs_skip.get() {
                    continue;
                }
                self.fs.step();
                self.fc = self.fc.wrapping_add(1);
            }
            return;
        }
        for _ in 0..n {
            if self.fs_skip.get() {
                self.fc = self.fc.wrapping_add(1);
                continue;
            }
            let p = self.ft.p;
            self.ch1.step(p);
            self.ch2.step(p);
            self.ch3.step(p);
            self.ch4.step(p);
            let s = self.fs.step();
            if Fseq::len(s) {
                if self.ch1.lc.step(&self.ch1.reg) {
                    self.ch1.on = false;
                }
                if self.ch2.lc.step(&self.ch2.reg) {
                    self.ch2.on = false;
                }
                if self.ch3.lc.step(&self.ch3.reg) {
                    self.ch3.on = false;
                }
                if self.ch4.lc.step(&self.ch4.reg) {
                    self.ch4.on = false;
                }
            }
            if Fseq::env(s) {
                if self.ch1.on {
                    self.ch1.ve.step(&self.ch1.reg);
                }
                if self.ch2.on {
                    self.ch2.ve.step(&self.ch2.reg);
                }
                if self.ch4.on {
                    self.ch4.ve.step(&self.ch4.reg);
                }
            }
            if Fseq::swp(s) && self.ch1.on {
                if self.ch1.fs.step(&mut self.ch1.reg) {
                    self.ch1.on = false;
                }
                self.ch1.tmr.p = self.ch1.reg.per_sq();
            }
            self.ch1.buf.end(p);
            self.ch2.buf.end(p);
            self.ch3.buf.end(p);
            self.ch4.buf.end(p);
            self.mix();
            self.fc = self.fc.wrapping_add(1);
        }
    }
}

// DMG wave-RAM corruption timing model (SameBoy-compatible).
//
// The internal sample-countdown is loaded with p1/2 + 2 2-MHz cycles after the first trigger and reset to p/2 − 1
// on every fire (where p is the current period in T-cycles). Re-triggering while sample_countdown == 0. Corrupts wave
// RAM at byte offset ((csi + 1) >> 1) & 0xF.
//
// p1_h  P1/2 (2-MHz cycles) at first trigger
// d1    2-MHz cycles between the first trigger and the last NR33 write
// d2    2-MHz cycles between that NR33 write and the re-trigger
// p2_h  P2/2 (2-MHz cycles) — the current period

fn dmg_corrupt_offset(p1_h: u32, d1: u32, d2: u32, p2_h: u32) -> Option<usize> {
    if p1_h == 0 || p2_h == 0 {
        return None;
    }
    let c1 = p1_h - 1;
    let c2 = p2_h - 1;
    let mut sc: u32 = c1 + 3;
    let mut csi: u32 = 0;
    // Phase 1.
    if sc >= d1 {
        sc -= d1;
    } else {
        let rem = d1 - (sc + 1);
        csi += 1;
        csi = csi.wrapping_add(rem / (c1 + 1)) & 0x1f;
        sc = c1.saturating_sub(rem % (c1 + 1));
    }
    // Phase 2.
    if sc >= d2 {
        sc -= d2;
    } else {
        let rem = d2 - (sc + 1);
        csi = (csi + 1) & 0x1f;
        csi = csi.wrapping_add(rem / (c2 + 1)) & 0x1f;
        sc = c2.saturating_sub(rem % (c2 + 1));
    }
    if sc == 0 { Some(((csi as usize + 1) >> 1) & 0xf) } else { None }
}
