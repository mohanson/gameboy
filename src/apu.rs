use super::clock::Clock;
use super::cpu;
use super::memory::Memory;
use blip_buf::BlipBuf;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

#[derive(Clone, Eq, PartialEq)]
enum Channel {
    Square1,
    Square2,
    Wave,
    Noise,
    Mixer,
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
struct Register {
    channel: Channel,
    nrx0: u8,
    nrx1: u8,
    nrx2: u8,
    nrx3: u8,
    nrx4: u8,
}

impl Register {
    fn get_sweep_period(&self) -> u8 {
        assert!(self.channel == Channel::Square1);
        (self.nrx0 >> 4) & 0x07
    }

    fn get_negate(&self) -> bool {
        assert!(self.channel == Channel::Square1);
        self.nrx0 & 0x08 != 0x00
    }

    fn get_shift(&self) -> u8 {
        assert!(self.channel == Channel::Square1);
        self.nrx0 & 0x07
    }

    fn get_dac_power(&self) -> bool {
        assert!(self.channel == Channel::Wave);
        self.nrx0 & 0x80 != 0x00
    }

    fn get_duty(&self) -> u8 {
        assert!(self.channel == Channel::Square1 || self.channel == Channel::Square2);
        self.nrx1 >> 6
    }

    fn get_length_load(&self) -> u16 {
        if self.channel == Channel::Wave {
            (1 << 8) - u16::from(self.nrx1)
        } else {
            (1 << 6) - u16::from(self.nrx1 & 0x3f)
        }
    }

    fn get_starting_volume(&self) -> u8 {
        assert!(self.channel != Channel::Wave);
        self.nrx2 >> 4
    }

    fn get_volume_code(&self) -> u8 {
        assert!(self.channel == Channel::Wave);
        (self.nrx2 >> 5) & 0x03
    }

    fn get_envelope_add_mode(&self) -> bool {
        assert!(self.channel != Channel::Wave);
        self.nrx2 & 0x08 != 0x00
    }

    fn get_period(&self) -> u8 {
        assert!(self.channel != Channel::Wave);
        self.nrx2 & 0x07
    }

    fn get_frequency(&self) -> u16 {
        assert!(self.channel != Channel::Noise);
        u16::from(self.nrx4 & 0x07) << 8 | u16::from(self.nrx3)
    }

    fn set_frequency(&mut self, f: u16) {
        assert!(self.channel != Channel::Noise);
        let h = ((f >> 8) & 0x07) as u8;
        let l = f as u8;
        self.nrx4 = (self.nrx4 & 0xf8) | h;
        self.nrx3 = l;
    }

    fn get_clock_shift(&self) -> u8 {
        assert!(self.channel == Channel::Noise);
        self.nrx3 >> 4
    }

    fn get_width_mode_of_lfsr(&self) -> bool {
        assert!(self.channel == Channel::Noise);
        self.nrx3 & 0x08 != 0x00
    }

    fn get_dividor_code(&self) -> u8 {
        assert!(self.channel == Channel::Noise);
        self.nrx3 & 0x07
    }

    fn get_trigger(&self) -> bool {
        self.nrx4 & 0x80 != 0x00
    }

    fn set_trigger(&mut self, b: bool) {
        if b {
            self.nrx4 |= 0x80;
        } else {
            self.nrx4 &= 0x7f;
        };
    }

    fn get_length_enable(&self) -> bool {
        self.nrx4 & 0x40 != 0x00
    }

    fn get_l_vol(&self) -> u8 {
        assert!(self.channel == Channel::Mixer);
        (self.nrx0 >> 4) & 0x07
    }

    fn get_r_vol(&self) -> u8 {
        assert!(self.channel == Channel::Mixer);
        self.nrx0 & 0x07
    }

    fn get_power(&self) -> bool {
        assert!(self.channel == Channel::Mixer);
        self.nrx2 & 0x80 != 0x00
    }
}

impl Register {
    fn power_up(channel: Channel) -> Self {
        let nrx1 = match channel {
            Channel::Square1 | Channel::Square2 => 0x40,
            _ => 0x00,
        };
        Self { channel, nrx0: 0x00, nrx1, nrx2: 0x00, nrx3: 0x00, nrx4: 0x00 }
    }
}

// Frame Sequencer
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
struct FrameSequencer {
    step: u8,
}

impl FrameSequencer {
    fn power_up() -> Self {
        Self { step: 0x00 }
    }

    fn next(&mut self) -> u8 {
        self.step += 1;
        self.step %= 8;
        self.step
    }
}

// A length counter disables a channel when it decrements to zero. It contains an internal counter and enabled flag.
// Writing a byte to NRx1 loads the counter with 64-data (256-data for wave channel). The counter can be reloaded at any
// time.
// A channel is said to be disabled when the internal enabled flag is clear. When a channel is disabled, its volume unit
// receives 0, otherwise its volume unit receives the output of the waveform generator. Other units besides the length
// counter can enable/disable the channel as well.
// Each length counter is clocked at 256 Hz by the frame sequencer. When clocked while enabled by NRx4 and the counter
// is not zero, it is decremented. If it becomes zero, the channel is disabled.
struct LengthCounter {
    reg: Rc<RefCell<Register>>,
    n: u16,
}

impl LengthCounter {
    fn power_up(reg: Rc<RefCell<Register>>) -> Self {
        Self { reg, n: 0x0000 }
    }

    fn next(&mut self) {
        if self.reg.borrow().get_length_enable() && self.n != 0 {
            self.n -= 1;
            if self.n == 0 {
                self.reg.borrow_mut().set_trigger(false);
            }
        }
    }

    fn reload(&mut self) {
        if self.n == 0x0000 {
            self.n = if self.reg.borrow().channel == Channel::Wave { 1 << 8 } else { 1 << 6 };
        }
    }
}

// A volume envelope has a volume counter and an internal timer clocked at 64 Hz by the frame sequencer. When the timer
// generates a clock and the envelope period is not zero, a new volume is calculated by adding or subtracting
// (as set by NRx2) one from the current volume. If this new volume within the 0 to 15 range, the volume is updated,
// otherwise it is left unchanged and no further automatic increments/decrements are made to the volume until the
// channel is triggered again.
// When the waveform input is zero the envelope outputs zero, otherwise it outputs the current volume.
// Writing to NRx2 causes obscure effects on the volume that differ on different Game Boy models (see obscure behavior).
struct VolumeEnvelope {
    reg: Rc<RefCell<Register>>,
    timer: Clock,
    volume: u8,
}

impl VolumeEnvelope {
    fn power_up(reg: Rc<RefCell<Register>>) -> Self {
        Self { reg, timer: Clock::power_up(8), volume: 0x00 }
    }

    fn reload(&mut self) {
        let p = self.reg.borrow().get_period();
        // The volume envelope and sweep timers treat a period of 0 as 8.
        self.timer.period = if p == 0 { 8 } else { u32::from(p) };
        self.volume = self.reg.borrow().get_starting_volume();
    }

    fn next(&mut self) {
        if self.reg.borrow().get_period() == 0 {
            return;
        }
        if self.timer.next(1) == 0x00 {
            return;
        };
        // If this new volume within the 0 to 15 range, the volume is updated
        let v = if self.reg.borrow().get_envelope_add_mode() {
            self.volume.wrapping_add(1)
        } else {
            self.volume.wrapping_sub(1)
        };
        if v <= 15 {
            self.volume = v;
        }
    }
}

// The first square channel has a frequency sweep unit, controlled by NR10. This has a timer, internal enabled flag,
// and frequency shadow register. It can periodically adjust square 1's frequency up or down.
// During a trigger event, several things occur:
//
//   - Square 1's frequency is copied to the shadow register.
//   - The sweep timer is reloaded.
//   - The internal enabled flag is set if either the sweep period or shift are non-zero, cleared otherwise.
//   - If the sweep shift is non-zero, frequency calculation and the overflow check are performed immediately.
//
// Frequency calculation consists of taking the value in the frequency shadow register, shifting it right by sweep
// shift, optionally negating the value, and summing this with the frequency shadow register to produce a new
// frequency. What is done with this new frequency depends on the context.
//
// The overflow check simply calculates the new frequency and if this is greater than 2047, square 1 is disabled.
// The sweep timer is clocked at 128 Hz by the frame sequencer. When it generates a clock and the sweep's internal
// enabled flag is set and the sweep period is not zero, a new frequency is calculated and the overflow check is
// performed. If the new frequency is 2047 or less and the sweep shift is not zero, this new frequency is written back
// to the shadow frequency and square 1's frequency in NR13 and NR14, then frequency calculation and overflow check are
// run AGAIN immediately using this new value, but this second new frequency is not written back.
// Square 1's frequency can be modified via NR13 and NR14 while sweep is active, but the shadow frequency won't be
// affected so the next time the sweep updates the channel's frequency this modification will be lost.
struct FrequencySweep {
    reg: Rc<RefCell<Register>>,
    timer: Clock,
    enable: bool,
    shadow: u16,
    newfeq: u16,
}

impl FrequencySweep {
    fn power_up(reg: Rc<RefCell<Register>>) -> Self {
        Self { reg, timer: Clock::power_up(8), enable: false, shadow: 0x0000, newfeq: 0x0000 }
    }

    fn reload(&mut self) {
        self.shadow = self.reg.borrow().get_frequency();
        let p = self.reg.borrow().get_sweep_period();
        // The volume envelope and sweep timers treat a period of 0 as 8.
        self.timer.period = if p == 0 { 8 } else { u32::from(p) };
        self.enable = p != 0x00 || self.reg.borrow().get_shift() != 0x00;
        if self.reg.borrow().get_shift() != 0x00 {
            self.frequency_calculation();
            self.overflow_check();
        }
    }

    fn frequency_calculation(&mut self) {
        let offset = self.shadow >> self.reg.borrow().get_shift();
        if self.reg.borrow().get_negate() {
            self.newfeq = self.shadow.wrapping_sub(offset);
        } else {
            self.newfeq = self.shadow.wrapping_add(offset);
        }
    }

    fn overflow_check(&mut self) {
        if self.newfeq >= 2048 {
            self.reg.borrow_mut().set_trigger(false);
        }
    }

    fn next(&mut self) {
        if !self.enable || self.reg.borrow().get_sweep_period() == 0 {
            return;
        }
        if self.timer.next(1) == 0x00 {
            return;
        }
        self.frequency_calculation();
        self.overflow_check();

        if self.newfeq < 2048 && self.reg.borrow().get_shift() != 0 {
            self.reg.borrow_mut().set_frequency(self.newfeq);
            self.shadow = self.newfeq;
            self.frequency_calculation();
            self.overflow_check();
        }
    }
}

struct Blip {
    data: BlipBuf,
    from: u32,
    ampl: i32,
}

impl Blip {
    fn power_up(data: BlipBuf) -> Self {
        Self { data, from: 0x0000_0000, ampl: 0x0000_0000 }
    }

    fn set(&mut self, time: u32, ampl: i32) {
        self.from = time;
        let d = ampl - self.ampl;
        self.ampl = ampl;
        self.data.add_delta(time, d);
    }
}

// A square channel's frequency timer period is set to (2048-frequency)*4. Four duty cycles are available, each
// waveform taking 8 frequency timer clocks to cycle through:
//
// Duty   Waveform    Ratio
// -------------------------
// 0      00000001    12.5%
// 1      10000001    25%
// 2      10000111    50%
// 3      01111110    75%
struct ChannelSquare {
    reg: Rc<RefCell<Register>>,
    timer: Clock,
    lc: LengthCounter,
    ve: VolumeEnvelope,
    fs: FrequencySweep,
    blip: Blip,
    idx: u8,
}

impl ChannelSquare {
    fn power_up(blip: BlipBuf, mode: Channel) -> ChannelSquare {
        let reg = Rc::new(RefCell::new(Register::power_up(mode.clone())));
        ChannelSquare {
            reg: reg.clone(),
            timer: Clock::power_up(8192),
            lc: LengthCounter::power_up(reg.clone()),
            ve: VolumeEnvelope::power_up(reg.clone()),
            fs: FrequencySweep::power_up(reg.clone()),
            blip: Blip::power_up(blip),
            idx: 1,
        }
    }

    // This assumes no volume or sweep adjustments need to be done in the meantime
    fn next(&mut self, cycles: u32) {
        let pat = match self.reg.borrow().get_duty() {
            0 => 0b0000_0001,
            1 => 0b1000_0001,
            2 => 0b1000_0111,
            3 => 0b0111_1110,
            _ => unreachable!(),
        };
        let vol = i32::from(self.ve.volume);
        for _ in 0..self.timer.next(cycles) {
            let ampl = if !self.reg.borrow().get_trigger() || self.ve.volume == 0 {
                0x00
            } else if (pat >> self.idx) & 0x01 != 0x00 {
                vol
            } else {
                vol * -1
            };
            self.blip.set(self.blip.from.wrapping_add(self.timer.period), ampl);
            self.idx = (self.idx + 1) % 8;
        }
    }
}

impl Memory for ChannelSquare {
    fn get(&self, a: u16) -> u8 {
        match a {
            0xff10 | 0xff15 => self.reg.borrow().nrx0,
            0xff11 | 0xff16 => self.reg.borrow().nrx1,
            0xff12 | 0xff17 => self.reg.borrow().nrx2,
            0xff13 | 0xff18 => self.reg.borrow().nrx3,
            0xff14 | 0xff19 => self.reg.borrow().nrx4,
            _ => unreachable!(),
        }
    }

    fn set(&mut self, a: u16, v: u8) {
        match a {
            0xff10 | 0xff15 => self.reg.borrow_mut().nrx0 = v,
            0xff11 | 0xff16 => {
                self.reg.borrow_mut().nrx1 = v;
                self.lc.n = self.reg.borrow().get_length_load();
            }
            0xff12 | 0xff17 => self.reg.borrow_mut().nrx2 = v,
            0xff13 | 0xff18 => {
                self.reg.borrow_mut().nrx3 = v;
                self.timer.period = period(self.reg.clone());
            }
            0xff14 | 0xff19 => {
                self.reg.borrow_mut().nrx4 = v;
                self.timer.period = period(self.reg.clone());
                // Trigger Event
                //
                // Writing a value to NRx4 with bit 7 set causes the following things to occur:
                //
                //   - Channel is enabled (see length counter).
                //   - If length counter is zero, it is set to 64 (256 for wave channel).
                //   - Frequency timer is reloaded with period.
                //   - Volume envelope timer is reloaded with period.
                //   - Channel volume is reloaded from NRx2.
                //   - Noise channel's LFSR bits are all set to 1.
                //   - Wave channel's position is set to 0 but sample buffer is NOT refilled.
                //   - Square 1's sweep does several things (see frequency sweep).
                //
                // Note that if the channel's DAC is off, after the above actions occur the channel will be immediately
                // disabled again.
                if self.reg.borrow().get_trigger() {
                    self.lc.reload();
                    self.ve.reload();
                    if self.reg.borrow().channel == Channel::Square1 {
                        self.fs.reload();
                    }
                }
            }
            _ => unreachable!(),
        }
    }
}

// The wave channel plays a 32-entry wave table made up of 4-bit samples. Each byte encodes two samples, the first in
// the high bits. The wave channel has a sample buffer and position counter.
// The wave channel's frequency timer period is set to (2048-frequency)*2. When the timer generates a clock, the
// position counter is advanced one sample in the wave table, looping back to the beginning when it goes past the end,
// then a sample is read into the sample buffer from this NEW position.
// The DAC receives the current value from the upper/lower nibble of the sample buffer, shifted right by the volume
// control.
//
// Code   Shift   Volume
// -----------------------
// 0      4         0% (silent)
// 1      0       100%
// 2      1        50%
// 3      2        25%
// Wave RAM can only be properly accessed when the channel is disabled (see obscure behavior).
struct ChannelWave {
    reg: Rc<RefCell<Register>>,
    timer: Clock,
    lc: LengthCounter,
    blip: Blip,
    waveram: [u8; 16],
    waveidx: usize,
}

impl ChannelWave {
    fn power_up(blip: BlipBuf) -> ChannelWave {
        let reg = Rc::new(RefCell::new(Register::power_up(Channel::Wave)));
        ChannelWave {
            reg: reg.clone(),
            timer: Clock::power_up(8192),
            lc: LengthCounter::power_up(reg.clone()),
            blip: Blip::power_up(blip),
            waveram: [0x00; 16],
            waveidx: 0x00,
        }
    }

    fn next(&mut self, cycles: u32) {
        let s = match self.reg.borrow().get_volume_code() {
            0 => 4,
            1 => 0,
            2 => 1,
            3 => 2,
            _ => unreachable!(),
        };
        for _ in 0..self.timer.next(cycles) {
            let sample = if self.waveidx & 0x01 == 0x00 {
                self.waveram[self.waveidx / 2] & 0x0f
            } else {
                self.waveram[self.waveidx / 2] >> 4
            };
            let ampl = if !self.reg.borrow().get_trigger() || !self.reg.borrow().get_dac_power() {
                0x00
            } else {
                i32::from(sample >> s)
            };
            self.blip.set(self.blip.from.wrapping_add(self.timer.period), ampl);
            self.waveidx = (self.waveidx + 1) % 32;
        }
    }
}

impl Memory for ChannelWave {
    fn get(&self, a: u16) -> u8 {
        match a {
            0xff1a => self.reg.borrow().nrx0,
            0xff1b => self.reg.borrow().nrx1,
            0xff1c => self.reg.borrow().nrx2,
            0xff1d => self.reg.borrow().nrx3,
            0xff1e => self.reg.borrow().nrx4,
            0xff30..=0xff3f => self.waveram[a as usize - 0xff30],
            _ => unreachable!(),
        }
    }

    fn set(&mut self, a: u16, v: u8) {
        match a {
            0xff1a => self.reg.borrow_mut().nrx0 = v,
            0xff1b => {
                self.reg.borrow_mut().nrx1 = v;
                self.lc.n = self.reg.borrow().get_length_load();
            }
            0xff1c => self.reg.borrow_mut().nrx2 = v,
            0xff1d => {
                self.reg.borrow_mut().nrx3 = v;
                self.timer.period = period(self.reg.clone());
            }
            0xff1e => {
                self.reg.borrow_mut().nrx4 = v;
                self.timer.period = period(self.reg.clone());
                if self.reg.borrow().get_trigger() {
                    self.lc.reload();
                    self.waveidx = 0x00;
                }
            }
            0xff30..=0xff3f => self.waveram[a as usize - 0xff30] = v,
            _ => unreachable!(),
        }
    }
}

// The linear feedback shift register (LFSR) generates a pseudo-random bit sequence. It has a 15-bit shift register
// with feedback. When clocked by the frequency timer, the low two bits (0 and 1) are XORed, all bits are shifted right
// by one, and the result of the XOR is put into the now-empty high bit. If width mode is 1 (NR43), the XOR result is
// ALSO put into bit 6 AFTER the shift, resulting in a 7-bit LFSR. The waveform output is bit 0 of the LFSR, INVERTED.
struct Lfsr {
    reg: Rc<RefCell<Register>>,
    n: u16,
}

impl Lfsr {
    fn power_up(reg: Rc<RefCell<Register>>) -> Self {
        Self { reg, n: 0x0001 }
    }

    fn next(&mut self) -> bool {
        let s = if self.reg.borrow().get_width_mode_of_lfsr() { 0x06 } else { 0x0e };
        let src = self.n;
        self.n <<= 1;
        let bit = ((src >> s) ^ (self.n >> s)) & 0x0001;
        self.n |= bit;
        (src >> s) & 0x0001 != 0x0000
    }

    fn reload(&mut self) {
        self.n = 0x0001
    }
}

struct ChannelNoise {
    reg: Rc<RefCell<Register>>,
    timer: Clock,
    lc: LengthCounter,
    ve: VolumeEnvelope,
    lfsr: Lfsr,
    blip: Blip,
}

impl ChannelNoise {
    fn power_up(blip: BlipBuf) -> ChannelNoise {
        let reg = Rc::new(RefCell::new(Register::power_up(Channel::Noise)));
        ChannelNoise {
            reg: reg.clone(),
            timer: Clock::power_up(4096),
            lc: LengthCounter::power_up(reg.clone()),
            ve: VolumeEnvelope::power_up(reg.clone()),
            lfsr: Lfsr::power_up(reg.clone()),
            blip: Blip::power_up(blip),
        }
    }

    fn next(&mut self, cycles: u32) {
        for _ in 0..self.timer.next(cycles) {
            let ampl = if !self.reg.borrow().get_trigger() || self.ve.volume == 0 {
                0x00
            } else if self.lfsr.next() {
                i32::from(self.ve.volume)
            } else {
                i32::from(self.ve.volume) * -1
            };
            self.blip.set(self.blip.from.wrapping_add(self.timer.period), ampl);
        }
    }
}

impl Memory for ChannelNoise {
    fn get(&self, a: u16) -> u8 {
        match a {
            0xff1f => self.reg.borrow().nrx0,
            0xff20 => self.reg.borrow().nrx1,
            0xff21 => self.reg.borrow().nrx2,
            0xff22 => self.reg.borrow().nrx3,
            0xff23 => self.reg.borrow().nrx4,
            _ => unreachable!(),
        }
    }

    fn set(&mut self, a: u16, v: u8) {
        match a {
            0xff1f => self.reg.borrow_mut().nrx0 = v,
            0xff20 => {
                self.reg.borrow_mut().nrx1 = v;
                self.lc.n = self.reg.borrow().get_length_load();
            }
            0xff21 => self.reg.borrow_mut().nrx2 = v,
            0xff22 => {
                self.reg.borrow_mut().nrx3 = v;
                self.timer.period = period(self.reg.clone());
            }
            0xff23 => {
                self.reg.borrow_mut().nrx4 = v;
                if self.reg.borrow().get_trigger() {
                    self.lc.reload();
                    self.ve.reload();
                    self.lfsr.reload();
                }
            }
            _ => unreachable!(),
        }
    }
}

pub struct Apu {
    pub buffer: Arc<Mutex<Vec<(f32, f32)>>>,
    reg: Register,
    timer: Clock,
    fs: FrameSequencer,
    channel1: ChannelSquare,
    channel2: ChannelSquare,
    channel3: ChannelWave,
    channel4: ChannelNoise,
    sample_rate: u32,
}

impl Apu {
    pub fn power_up(sample_rate: u32) -> Self {
        let blipbuf1 = create_blipbuf(sample_rate);
        let blipbuf2 = create_blipbuf(sample_rate);
        let blipbuf3 = create_blipbuf(sample_rate);
        let blipbuf4 = create_blipbuf(sample_rate);
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            reg: Register::power_up(Channel::Mixer),
            timer: Clock::power_up(cpu::CLOCK_FREQUENCY / 512),
            fs: FrameSequencer::power_up(),
            channel1: ChannelSquare::power_up(blipbuf1, Channel::Square1),
            channel2: ChannelSquare::power_up(blipbuf2, Channel::Square2),
            channel3: ChannelWave::power_up(blipbuf3),
            channel4: ChannelNoise::power_up(blipbuf4),
            sample_rate,
        }
    }

    fn play(&mut self, l: &[f32], r: &[f32]) {
        assert_eq!(l.len(), r.len());
        let mut buffer = self.buffer.lock().unwrap();
        for (l, r) in l.iter().zip(r) {
            // Do not fill the buffer with more than 1 second of data
            // This speeds up the resync after the turning on and off the speed limiter
            if buffer.len() > self.sample_rate as usize {
                return;
            }
            buffer.push((*l, *r));
        }
    }

    pub fn next(&mut self, cycles: u32) {
        if !self.reg.get_power() {
            return;
        }

        for _ in 0..self.timer.next(cycles) {
            self.channel1.next(self.timer.period);
            self.channel2.next(self.timer.period);
            self.channel3.next(self.timer.period);
            self.channel4.next(self.timer.period);

            let step = self.fs.next();
            if step == 0 || step == 2 || step == 4 || step == 6 {
                self.channel1.lc.next();
                self.channel2.lc.next();
                self.channel3.lc.next();
                self.channel4.lc.next();
            }
            if step == 7 {
                self.channel1.ve.next();
                self.channel2.ve.next();
                self.channel4.ve.next();
            }
            if step == 2 || step == 6 {
                self.channel1.fs.next();
                self.channel1.timer.period = period(self.channel1.reg.clone());
            }

            self.channel1.blip.data.end_frame(self.timer.period);
            self.channel2.blip.data.end_frame(self.timer.period);
            self.channel3.blip.data.end_frame(self.timer.period);
            self.channel4.blip.data.end_frame(self.timer.period);
            self.channel1.blip.from = self.channel1.blip.from.wrapping_sub(self.timer.period);
            self.channel2.blip.from = self.channel2.blip.from.wrapping_sub(self.timer.period);
            self.channel3.blip.from = self.channel3.blip.from.wrapping_sub(self.timer.period);
            self.channel4.blip.from = self.channel4.blip.from.wrapping_sub(self.timer.period);
            self.mix();
        }
    }

    fn mix(&mut self) {
        let sc1 = self.channel1.blip.data.samples_avail();
        let sc2 = self.channel2.blip.data.samples_avail();
        let sc3 = self.channel3.blip.data.samples_avail();
        let sc4 = self.channel4.blip.data.samples_avail();
        assert_eq!(sc1, sc2);
        assert_eq!(sc2, sc3);
        assert_eq!(sc3, sc4);

        let sample_count = sc1 as usize;
        let mut sum = 0;

        let l_vol = (f32::from(self.reg.get_l_vol()) / 7.0) * (1.0 / 15.0) * 0.25;
        let r_vol = (f32::from(self.reg.get_r_vol()) / 7.0) * (1.0 / 15.0) * 0.25;

        while sum < sample_count {
            let buf_l = &mut [0f32; 2048];
            let buf_r = &mut [0f32; 2048];
            let buf = &mut [0i16; 2048];

            let count1 = self.channel1.blip.data.read_samples(buf, false);
            for (i, v) in buf[..count1].iter().enumerate() {
                if self.reg.nrx1 & 0x01 == 0x01 {
                    buf_l[i] += f32::from(*v) * l_vol;
                }
                if self.reg.nrx1 & 0x10 == 0x10 {
                    buf_r[i] += f32::from(*v) * r_vol;
                }
            }

            let count2 = self.channel2.blip.data.read_samples(buf, false);
            for (i, v) in buf[..count2].iter().enumerate() {
                if self.reg.nrx1 & 0x02 == 0x02 {
                    buf_l[i] += f32::from(*v) * l_vol;
                }
                if self.reg.nrx1 & 0x20 == 0x20 {
                    buf_r[i] += f32::from(*v) * r_vol;
                }
            }

            let count3 = self.channel3.blip.data.read_samples(buf, false);
            for (i, v) in buf[..count3].iter().enumerate() {
                if self.reg.nrx1 & 0x04 == 0x04 {
                    buf_l[i] += f32::from(*v) * l_vol;
                }
                if self.reg.nrx1 & 0x40 == 0x40 {
                    buf_r[i] += f32::from(*v) * r_vol;
                }
            }

            let count4 = self.channel4.blip.data.read_samples(buf, false);
            for (i, v) in buf[..count4].iter().enumerate() {
                if self.reg.nrx1 & 0x08 == 0x08 {
                    buf_l[i] += f32::from(*v) * l_vol;
                }
                if self.reg.nrx1 & 0x80 == 0x80 {
                    buf_r[i] += f32::from(*v) * r_vol;
                }
            }

            assert_eq!(count1, count2);
            assert_eq!(count2, count3);
            assert_eq!(count3, count4);

            self.play(&buf_l[..count1], &buf_r[..count1]);
            sum += count1;
        }
    }
}

// Registers are ORed with this when reading
const RD_MASK: [u8; 48] = [
    0x80, 0x3f, 0x00, 0xff, 0xbf, 0xff, 0x3f, 0x00, 0xff, 0xbf, 0x7f, 0xff, 0x9f, 0xff, 0xbf, 0xff, 0xff, 0x00, 0x00,
    0xbf, 0x00, 0x00, 0x70, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

impl Memory for Apu {
    fn get(&self, a: u16) -> u8 {
        let r = match a {
            0xff10..=0xff14 => self.channel1.get(a),
            0xff15..=0xff19 => self.channel2.get(a),
            0xff1a..=0xff1e => self.channel3.get(a),
            0xff1f..=0xff23 => self.channel4.get(a),
            0xff24 => self.reg.nrx0,
            0xff25 => self.reg.nrx1,
            0xff26 => {
                let a = self.reg.nrx2 & 0xf0;
                let b = if self.channel1.reg.borrow().get_trigger() { 1 } else { 0 };
                let c = if self.channel2.reg.borrow().get_trigger() { 2 } else { 0 };
                let d = if self.channel3.reg.borrow().get_trigger() && self.channel3.reg.borrow().get_dac_power() {
                    4
                } else {
                    0
                };
                let e = if self.channel4.reg.borrow().get_trigger() { 8 } else { 0 };
                a | b | c | d | e
            }
            0xff27..=0xff2f => 0x00,
            0xff30..=0xff3f => self.channel3.get(a),
            _ => unreachable!(),
        };
        r | RD_MASK[a as usize - 0xff10]
    }

    fn set(&mut self, a: u16, v: u8) {
        if a != 0xff26 && !self.reg.get_power() {
            return;
        }
        match a {
            0xff10..=0xff14 => self.channel1.set(a, v),
            0xff15..=0xff19 => self.channel2.set(a, v),
            0xff1a..=0xff1e => self.channel3.set(a, v),
            0xff1f..=0xff23 => self.channel4.set(a, v),
            0xff24 => self.reg.nrx0 = v,
            0xff25 => self.reg.nrx1 = v,
            0xff26 => {
                self.reg.nrx2 = v;
                // Powering APU off should write 0 to all regs
                // Powering APU off shouldn't affect wave, that wave RAM is unchanged
                if !self.reg.get_power() {
                    self.channel1.reg.borrow_mut().nrx0 = 0x00;
                    self.channel1.reg.borrow_mut().nrx1 = 0x00;
                    self.channel1.reg.borrow_mut().nrx2 = 0x00;
                    self.channel1.reg.borrow_mut().nrx3 = 0x00;
                    self.channel1.reg.borrow_mut().nrx4 = 0x00;
                    self.channel2.reg.borrow_mut().nrx0 = 0x00;
                    self.channel2.reg.borrow_mut().nrx1 = 0x00;
                    self.channel2.reg.borrow_mut().nrx2 = 0x00;
                    self.channel2.reg.borrow_mut().nrx3 = 0x00;
                    self.channel2.reg.borrow_mut().nrx4 = 0x00;
                    self.channel3.reg.borrow_mut().nrx0 = 0x00;
                    self.channel3.reg.borrow_mut().nrx1 = 0x00;
                    self.channel3.reg.borrow_mut().nrx2 = 0x00;
                    self.channel3.reg.borrow_mut().nrx3 = 0x00;
                    self.channel3.reg.borrow_mut().nrx4 = 0x00;
                    self.channel4.reg.borrow_mut().nrx0 = 0x00;
                    self.channel4.reg.borrow_mut().nrx1 = 0x00;
                    self.channel4.reg.borrow_mut().nrx2 = 0x00;
                    self.channel4.reg.borrow_mut().nrx3 = 0x00;
                    self.channel4.reg.borrow_mut().nrx4 = 0x00;
                    self.reg.nrx0 = 0x00;
                    self.reg.nrx1 = 0x00;
                    self.reg.nrx2 = 0x00;
                    self.reg.nrx3 = 0x00;
                    self.reg.nrx4 = 0x00;
                }
            }
            0xff27..=0xff2f => {}
            0xff30..=0xff3f => self.channel3.set(a, v),
            _ => unreachable!(),
        }
    }
}

fn create_blipbuf(sample_rate: u32) -> BlipBuf {
    let mut blipbuf = BlipBuf::new(sample_rate);
    blipbuf.set_rates(f64::from(cpu::CLOCK_FREQUENCY), f64::from(sample_rate));
    blipbuf
}

fn period(reg: Rc<RefCell<Register>>) -> u32 {
    match reg.borrow().channel {
        Channel::Square1 | Channel::Square2 => 4 * (2048 - u32::from(reg.borrow().get_frequency())),
        Channel::Wave => 2 * (2048 - u32::from(reg.borrow().get_frequency())),
        Channel::Noise => {
            let d = match reg.borrow().get_dividor_code() {
                0 => 8,
                n => (u32::from(n) + 1) * 16,
            };
            d << reg.borrow().get_clock_shift()
        }
        Channel::Mixer => cpu::CLOCK_FREQUENCY / 512,
    }
}
