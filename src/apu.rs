use super::convention::{CLOCK_FREQUENCY, Memory, Term};
use blip_buf::BlipBuf;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

// Clock is outputed 1 cycle every N cycles.
pub struct Clock {
    pub period: u32,
    pub n: u32,
}

impl Clock {
    pub fn power_up(period: u32) -> Self {
        Self { period, n: 0x00 }
    }

    pub fn next(&mut self, cycles: u32) -> u32 {
        self.n += cycles;
        let rs = self.n / self.period;
        self.n = self.n % self.period;
        rs
    }
}

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
    // Obscure behavior: tracks whether at least one frequency calculation using
    // subtraction (negate) mode has been performed since the last trigger.  When
    // the negate bit in NR10 is later cleared (1→0), CH1 is immediately disabled.
    negated_since_trigger: bool,
}

impl FrequencySweep {
    fn power_up(reg: Rc<RefCell<Register>>) -> Self {
        Self { reg, timer: Clock::power_up(8), enable: false, shadow: 0x0000, newfeq: 0x0000, negated_since_trigger: false }
    }

    fn reload(&mut self) {
        self.shadow = self.reg.borrow().get_frequency();
        let p = self.reg.borrow().get_sweep_period();
        // The volume envelope and sweep timers treat a period of 0 as 8.
        self.timer.period = if p == 0 { 8 } else { u32::from(p) };
        self.enable = p != 0x00 || self.reg.borrow().get_shift() != 0x00;
        self.negated_since_trigger = false;
        if self.reg.borrow().get_shift() != 0x00 {
            self.frequency_calculation();
            self.overflow_check();
        }
    }

    fn frequency_calculation(&mut self) {
        let offset = self.shadow >> self.reg.borrow().get_shift();
        if self.reg.borrow().get_negate() {
            self.newfeq = self.shadow.wrapping_sub(offset);
            self.negated_since_trigger = true;
        } else {
            self.newfeq = self.shadow.wrapping_add(offset);
        }
    }

    // Called when NR10 is written.  Returns true if CH1 should be disabled due
    // to the obscure "negate-disable" behavior (negate bit cleared after at
    // least one subtraction sweep calculation since the last trigger).
    fn sb_nr10(&mut self, v: u8) -> bool {
        let old_negate = self.reg.borrow().get_negate();
        self.reg.borrow_mut().nrx0 = v;
        let new_negate = self.reg.borrow().get_negate();
        old_negate && !new_negate && self.negated_since_trigger
    }

    fn overflow_check(&mut self) {
        if self.newfeq >= 2048 {
            self.reg.borrow_mut().set_trigger(false);
        }
    }

    fn next(&mut self) {
        let did_tick = self.timer.next(1) != 0;
        // On every timer fire, reload the period from the current NR10 register
        // (treating 0 as 8).  This is the correct hardware behaviour: the sweep
        // timer reloads from the register each time it expires, so writes to NR10
        // between fires take effect on the next reload.
        if did_tick {
            let p = self.reg.borrow().get_sweep_period();
            self.timer.period = if p == 0 { 8 } else { u32::from(p) };
            self.timer.n = 0;
        }
        if !self.enable || self.reg.borrow().get_sweep_period() == 0 {
            return;
        }
        if !did_tick {
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

    // NRx4 write with optional extra length clock.
    // extra_clock=true when the frame sequencer just fired an even step (0,2,4,6), i.e. we
    // are in the "first half" of the length period.  In that case, enabling the length
    // counter (bit6 0→1) produces one extra decrement, and a trigger when length was
    // already zero causes the reloaded max to be decremented by one.
    fn sb_nrx4(&mut self, v: u8, extra_clock: bool) {
        let old = self.reg.borrow().nrx4;
        let old_enable = old & 0x40 != 0;
        let new_enable = v & 0x40 != 0;
        let trigger = v & 0x80 != 0;

        self.reg.borrow_mut().nrx4 = (old & 0x80) | (v & 0x7f);
        self.timer.period = period(self.reg.clone());

        // Extra length clock when enable goes 0→1 in first half of length period.
        if extra_clock && !old_enable && new_enable && self.lc.n != 0 {
            self.lc.n -= 1;
            if self.lc.n == 0 {
                self.reg.borrow_mut().set_trigger(false);
            }
        }

        if trigger {
            let len_was_zero = self.lc.n == 0;
            self.reg.borrow_mut().nrx4 |= 0x80;
            self.lc.reload();
            self.ve.reload();
            if self.reg.borrow().channel == Channel::Square1 {
                self.fs.reload();
            }
            // If enable bit is set in this write and length was 0 (after possible extra
            // enable clock), the reloaded max should be decremented by one.
            if extra_clock && new_enable && len_was_zero && self.lc.n != 0 {
                self.lc.n -= 1;
            }
            if self.reg.borrow().nrx2 & 0xf8 == 0x00 {
                self.reg.borrow_mut().set_trigger(false);
            }
        }
    }
}

impl Memory for ChannelSquare {
    fn lb(&self, a: u16) -> u8 {
        match a {
            0xff10 | 0xff15 => self.reg.borrow().nrx0,
            0xff11 | 0xff16 => self.reg.borrow().nrx1,
            0xff12 | 0xff17 => self.reg.borrow().nrx2,
            0xff13 | 0xff18 => self.reg.borrow().nrx3,
            0xff14 | 0xff19 => self.reg.borrow().nrx4,
            _ => unreachable!(),
        }
    }

    fn sb(&mut self, a: u16, v: u8) {
        match a {
            0xff10 => {
                if self.fs.sb_nr10(v) {
                    self.reg.borrow_mut().set_trigger(false);
                }
            }
            0xff15 => self.reg.borrow_mut().nrx0 = v,
            0xff11 | 0xff16 => {
                self.reg.borrow_mut().nrx1 = v;
                self.lc.n = self.reg.borrow().get_length_load();
            }
            0xff12 | 0xff17 => {
                self.reg.borrow_mut().nrx2 = v;
                // DAC off (starting volume=0 and not adding) → immediately disable channel.
                if v & 0xf8 == 0x00 {
                    self.reg.borrow_mut().set_trigger(false);
                }
            }
            0xff13 | 0xff18 => {
                self.reg.borrow_mut().nrx3 = v;
                self.timer.period = period(self.reg.clone());
            }
            0xff14 | 0xff19 => self.sb_nrx4(v, false),
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
    #[allow(dead_code)]
    term: Term,
    // DMG wave-RAM corruption timing state.
    // trigger_sdiv: sdiv at the last first-trigger (was_active=false).
    trigger_sdiv: u16,
    // p1_half: (2048 - freq) at first trigger = P1 period in 2MHz cycles.
    p1_half: u32,
    // d1_2mhz: 2MHz cycles elapsed from first trigger to last NR33 write (while active).
    d1_2mhz: u32,
    // Live-waveidx tracking: record the APU frame counter and APU frame timer
    // accumulator at the time of the last trigger so we can compute the real
    // wave position for wave-RAM reads without advancing the audio state.
    trigger_frame: u32,
    trigger_apu_n: u32,
}

impl ChannelWave {
    fn power_up(blip: BlipBuf, term: Term) -> ChannelWave {
        let reg = Rc::new(RefCell::new(Register::power_up(Channel::Wave)));
        ChannelWave {
            reg: reg.clone(),
            timer: Clock::power_up(8192),
            lc: LengthCounter::power_up(reg.clone()),
            blip: Blip::power_up(blip),
            waveram: [0x00; 16],
            waveidx: 0x00,
            term,
            trigger_sdiv: 0,
            p1_half: 0,
            d1_2mhz: 0,
            trigger_frame: 0,
            trigger_apu_n: 0,
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

    // Apply DMG wave-RAM corruption that occurs when CH3 is re-triggered while playing.
    // `byte_offset` is the wave-RAM byte index read by the hardware at the trigger moment,
    // matching SameBoy's `((current_sample_index + 1) >> 1) & 0xF` formula.
    fn apply_dmg_wave_corruption(&mut self, byte_offset: usize) {
        if byte_offset < 4 {
            self.waveram[0] = self.waveram[byte_offset];
        } else {
            let aligned = byte_offset & !3;
            self.waveram.copy_within(aligned..aligned + 4, 0);
        }
    }

    fn sb_nrx4(&mut self, v: u8, extra_clock: bool, dmg_corruption_offset: Option<usize>) {
        let old = self.reg.borrow().nrx4;
        let old_enable = old & 0x40 != 0;
        let new_enable = v & 0x40 != 0;
        let trigger = v & 0x80 != 0;

        self.reg.borrow_mut().nrx4 = (old & 0x80) | (v & 0x7f);
        self.timer.period = period(self.reg.clone());

        if extra_clock && !old_enable && new_enable && self.lc.n != 0 {
            self.lc.n -= 1;
            if self.lc.n == 0 {
                self.reg.borrow_mut().set_trigger(false);
            }
        }

        if trigger {
            // DMG obscure behavior: triggering CH3 while it is active corrupts wave RAM.
            // The corruption offset (byte index) is pre-computed by the Apu-level caller
            // using the SameBoy-accurate sample_countdown==0 timing model.
            if let Some(offset) = dmg_corruption_offset {
                self.apply_dmg_wave_corruption(offset);
            }

            let len_was_zero = self.lc.n == 0;
            self.reg.borrow_mut().nrx4 |= 0x80;
            self.lc.reload();
            // Per spec, triggering CH3 reloads the period divider (resets the internal
            // countdown to zero so the first nibble fires after a full period).
            self.timer.n = 0;
            self.waveidx = 0x00;
            if extra_clock && new_enable && len_was_zero && self.lc.n != 0 {
                self.lc.n -= 1;
            }
            if !self.reg.borrow().get_dac_power() {
                self.reg.borrow_mut().set_trigger(false);
            }
        }
    }
}

impl Memory for ChannelWave {
    fn lb(&self, a: u16) -> u8 {
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

    fn sb(&mut self, a: u16, v: u8) {
        match a {
            0xff1a => {
                self.reg.borrow_mut().nrx0 = v;
                // DAC off (bit 7 = 0) → immediately disable channel.
                if v & 0x80 == 0x00 {
                    self.reg.borrow_mut().set_trigger(false);
                }
            }
            0xff1b => {
                self.reg.borrow_mut().nrx1 = v;
                self.lc.n = self.reg.borrow().get_length_load();
            }
            0xff1c => self.reg.borrow_mut().nrx2 = v,
            0xff1d => {
                self.reg.borrow_mut().nrx3 = v;
                self.timer.period = period(self.reg.clone());
            }
            0xff1e => self.sb_nrx4(v, false, None),
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

    fn sb_nrx4(&mut self, v: u8, extra_clock: bool) {
        let old = self.reg.borrow().nrx4;
        let old_enable = old & 0x40 != 0;
        let new_enable = v & 0x40 != 0;
        let trigger = v & 0x80 != 0;

        self.reg.borrow_mut().nrx4 = (old & 0x80) | (v & 0x7f);

        if extra_clock && !old_enable && new_enable && self.lc.n != 0 {
            self.lc.n -= 1;
            if self.lc.n == 0 {
                self.reg.borrow_mut().set_trigger(false);
            }
        }

        if trigger {
            let len_was_zero = self.lc.n == 0;
            self.reg.borrow_mut().nrx4 |= 0x80;
            self.lc.reload();
            self.ve.reload();
            self.lfsr.reload();
            if extra_clock && new_enable && len_was_zero && self.lc.n != 0 {
                self.lc.n -= 1;
            }
            if self.reg.borrow().nrx2 & 0xf8 == 0x00 {
                self.reg.borrow_mut().set_trigger(false);
            }
        }
    }
}

impl Memory for ChannelNoise {
    fn lb(&self, a: u16) -> u8 {
        match a {
            0xff1f => self.reg.borrow().nrx0,
            0xff20 => self.reg.borrow().nrx1,
            0xff21 => self.reg.borrow().nrx2,
            0xff22 => self.reg.borrow().nrx3,
            0xff23 => self.reg.borrow().nrx4,
            _ => unreachable!(),
        }
    }

    fn sb(&mut self, a: u16, v: u8) {
        match a {
            0xff1f => self.reg.borrow_mut().nrx0 = v,
            0xff20 => {
                self.reg.borrow_mut().nrx1 = v;
                self.lc.n = self.reg.borrow().get_length_load();
            }
            0xff21 => {
                self.reg.borrow_mut().nrx2 = v;
                // DAC off → immediately disable channel.
                if v & 0xf8 == 0x00 {
                    self.reg.borrow_mut().set_trigger(false);
                }
            }
            0xff22 => {
                self.reg.borrow_mut().nrx3 = v;
                self.timer.period = period(self.reg.clone());
            }
            0xff23 => self.sb_nrx4(v, false),
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
    // DIV-APU synchronization: updated from mmu before every APU register write
    // so the NR52 power-on handler can phase-align the frame sequencer to the
    // hardware DIV counter.
    pub sdiv_cache: u16,
    // When the APU is powered on while sdiv bit 12 is high, the hardware skips
    // the very first DIV-APU event (SameBoy "APU glitch").
    skip_next_fs_tick: bool,
    // DMG vs CGB: on DMG, NR11/NR21/NR31/NR41 are writable when APU is off
    // (length counters can be loaded), and lc.n is preserved through power-on.
    // On CGB, all channel writes are blocked when APU is off.
    term: Term,
    // Monotonically-increasing counter incremented once per 8192-T-cycle APU frame.
    // Used together with trigger_frame/trigger_apu_n in ChannelWave to compute the
    // live wave position for wave-RAM reads without disturbing the audio state.
    frame_count: u32,
}

impl Apu {
    pub fn power_up(sample_rate: u32, term: Term) -> Self {
        let blipbuf1 = create_blipbuf(sample_rate);
        let blipbuf2 = create_blipbuf(sample_rate);
        let blipbuf3 = create_blipbuf(sample_rate);
        let blipbuf4 = create_blipbuf(sample_rate);
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            reg: Register::power_up(Channel::Mixer),
            timer: Clock::power_up(CLOCK_FREQUENCY / 512),
            fs: FrameSequencer::power_up(),
            channel1: ChannelSquare::power_up(blipbuf1, Channel::Square1),
            channel2: ChannelSquare::power_up(blipbuf2, Channel::Square2),
            channel3: ChannelWave::power_up(blipbuf3, term),
            channel4: ChannelNoise::power_up(blipbuf4),
            sample_rate,
            sdiv_cache: 0,
            skip_next_fs_tick: false,
            term,
            frame_count: 0,
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
        // Count ticks first so the timer state is consumed regardless of power state.
        let ticks = self.timer.next(cycles);

        if !self.reg.get_power() {
            // The frame sequencer (DIV-APU) keeps running even when the APU is powered off.
            for _ in 0..ticks {
                if self.skip_next_fs_tick {
                    self.skip_next_fs_tick = false;
                    continue;
                }
                self.fs.next();
            }
            return;
        }

        for _ in 0..ticks {
            if self.skip_next_fs_tick {
                self.skip_next_fs_tick = false;
                continue;
            }
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
            self.frame_count = self.frame_count.wrapping_add(1);
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
    fn lb(&self, a: u16) -> u8 {
        let r = match a {
            0xff10..=0xff14 => self.channel1.lb(a),
            0xff15..=0xff19 => self.channel2.lb(a),
            0xff1a..=0xff1e => self.channel3.lb(a),
            0xff1f..=0xff23 => self.channel4.lb(a),
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
            0xff30..=0xff3f => {
                // While CH3 is active, the CPU can only access the byte currently being
                // read by the wave hardware (DMG: only in the 2-cycle window after a read;
                // CGB: always). We redirect to the current-position byte for both models,
                // which is correct for CGB and matches the window blargg DMG tests use.
                //
                // Since we batch-process the APU every 8192 T-cycles, waveidx is frozen
                // between APU frames. We compute the "live" waveidx by counting extra fires
                // from the time the wave channel was last processed (or triggered) using the
                // APU frame-timer accumulator (self.timer.n).
                let is_active = self.channel3.reg.borrow().get_trigger()
                    && self.channel3.reg.borrow().get_dac_power();
                if is_active {
                    // Compute the "live" wave position by counting all fires since the last
                    // trigger, regardless of how many APU frames have elapsed.
                    //
                    // elapsed = full APU frames * 8192  +  timer.n  -  trigger_apu_n
                    //   (works for D=0 when timer.n >= trigger_apu_n, and D>=1 in general)
                    //
                    // SameBoy's initial countdown = (period/2 - 1) + 3 in 2MHz cycles, so
                    // the first advance fires at (trigger_period + 6) T-cycles after trigger.
                    // After the first fire, each subsequent advance uses the CURRENT timer
                    // period (which may differ if NR33 was written between trigger and read).
                    // p1_half = trigger_period / 2 (recorded at trigger time).
                    let wave_period = self.channel3.timer.period as u64; // current (post-NR33) period
                    let trigger_period = self.channel3.p1_half as u64 * 2; // period AT trigger
                    let d = self.frame_count.wrapping_sub(self.channel3.trigger_frame) as u64;
                    let elapsed = d * self.timer.period as u64
                        + self.timer.n as u64
                        - self.channel3.trigger_apu_n as u64;
                    let t_first = trigger_period + 6; // first advance at trigger_period + 6 T-cycles
                    let fires = if elapsed < t_first {
                        0u64
                    } else {
                        1 + (elapsed - t_first) / wave_period
                    };
                    let live_waveidx = (fires % 32) as usize;
                    // DMG: wave RAM is only readable in a ~2-T-cycle window right after a
                    // wave advance.  Outside that window (or before the first advance),
                    // the read returns 0xFF.
                    if self.term == Term::DMG {
                        if fires == 0 {
                            0xff
                        } else {
                            let t_last = t_first + (fires - 1) * wave_period;
                            let dist = elapsed - t_last;
                            if dist < 2 {
                                self.channel3.waveram[live_waveidx / 2]
                            } else {
                                0xff
                            }
                        }
                    } else {
                        self.channel3.waveram[live_waveidx / 2]
                    }
                } else {
                    self.channel3.lb(a)
                }
            }
            _ => unreachable!(),
        };
        r | RD_MASK[a as usize - 0xff10]
    }

    fn sb(&mut self, a: u16, v: u8) {
        // Wave RAM (0xFF30-0xFF3F) is always accessible regardless of APU power state.
        // All other registers (except NR52 = 0xFF26) are blocked when APU is off.
        if a != 0xff26 && !(0xff30..=0xff3f).contains(&a) && !self.reg.get_power() {
            // On DMG, NR11/NR21/NR31/NR41 (length counter registers) are
            // writable even when the APU is powered off.  This allows games
            // to pre-load length counters before enabling the APU.
            // On CGB all channel writes are blocked when APU is off.
            if self.term == Term::DMG {
                match a {
                    // NR11 / NR21: update lc.n; store only lower 6 bits
                    // (duty-cycle field in bits 7-6 is NOT written when off)
                    0xff11 => {
                        self.channel1.reg.borrow_mut().nrx1 = v & 0x3f;
                        self.channel1.lc.n = self.channel1.reg.borrow().get_length_load();
                        return;
                    }
                    0xff16 => {
                        self.channel2.reg.borrow_mut().nrx1 = v & 0x3f;
                        self.channel2.lc.n = self.channel2.reg.borrow().get_length_load();
                        return;
                    }
                    // NR31: full value stored, update lc.n
                    0xff1b => {
                        self.channel3.reg.borrow_mut().nrx1 = v;
                        self.channel3.lc.n = self.channel3.reg.borrow().get_length_load();
                        return;
                    }
                    // NR41: update lc.n
                    0xff20 => {
                        self.channel4.reg.borrow_mut().nrx1 = v;
                        self.channel4.lc.n = self.channel4.reg.borrow().get_length_load();
                        return;
                    }
                    _ => {}
                }
            }
            return;
        }
        // For NRx4 writes, pass the extra_clock flag based on the current FS step.
        // extra_clock is true when the FS just fired an even step (0,2,4,6) — the
        // "first half" of the length period per the GB APU obscure behaviour spec.
        let extra_clock = self.fs.step % 2 == 0;
        match a {
            0xff10..=0xff13 => self.channel1.sb(a, v),
            0xff14 => self.channel1.sb_nrx4(v, extra_clock),
            0xff15..=0xff18 => self.channel2.sb(a, v),
            0xff19 => self.channel2.sb_nrx4(v, extra_clock),
            0xff1a..=0xff1c => self.channel3.sb(a, v),
            0xff1d => {
                // On DMG: record d1_2mhz (2MHz cycles from first trigger to this NR33 write)
                // for use in the SameBoy-accurate wave corruption model.
                if self.term == Term::DMG {
                    let is_active = self.channel3.reg.borrow().get_trigger()
                        && self.channel3.reg.borrow().get_dac_power();
                    if is_active {
                        let elapsed_t = self.sdiv_cache.wrapping_sub(self.channel3.trigger_sdiv) as u32;
                        self.channel3.d1_2mhz = elapsed_t / 2;
                    }
                }
                self.channel3.sb(a, v);
            }
            0xff1e => {
                let sdiv = self.sdiv_cache;
                let was_active = self.channel3.reg.borrow().get_trigger()
                    && self.channel3.reg.borrow().get_dac_power();
                // On DMG, compute whether corruption happens using the SameBoy model:
                // corruption fires only when sample_countdown == 0 at the exact trigger moment.
                // The SameBoy wave timer starts at (P1/2 + 2) in 2MHz cycles after trigger
                // and resets to (P/2 - 1) after each fire (where P is the current period in T-cycles).
                let dmg_corruption_offset = if v & 0x80 != 0 && was_active && self.term == Term::DMG {
                    let elapsed_t = sdiv.wrapping_sub(self.channel3.trigger_sdiv) as u32;
                    let elapsed_2mhz = elapsed_t / 2;
                    let d1_2mhz = self.channel3.d1_2mhz;
                    let d2_2mhz = elapsed_2mhz.saturating_sub(d1_2mhz);
                    let p1_half = self.channel3.p1_half;
                    // Current period in 2MHz cycles (P2/2 = 2048 - freq).
                    // timer.period was already updated by any prior NR33 write.
                    let p2_half = self.channel3.timer.period / 2;
                    compute_wave_corruption_offset(p1_half, d1_2mhz, d2_2mhz, p2_half)
                } else {
                    None
                };
                // Record trigger state for next re-trigger.
                if v & 0x80 != 0 {
                    if !was_active {
                        // First trigger: save p1_half from the current (pre-write) period.
                        // timer.period was updated by the preceding NR33 write if any.
                        self.channel3.trigger_sdiv = sdiv;
                        self.channel3.p1_half = self.channel3.timer.period / 2;
                        self.channel3.d1_2mhz = 0;
                    } else {
                        // Re-trigger: update trigger_sdiv so the NEXT re-trigger can use it.
                        self.channel3.trigger_sdiv = sdiv;
                        self.channel3.p1_half = self.channel3.timer.period / 2;
                        self.channel3.d1_2mhz = 0;
                    }
                    // Record APU frame state so Apu::lb can compute the live wave position.
                    self.channel3.trigger_apu_n = self.timer.n;
                    self.channel3.trigger_frame = self.frame_count;
                }
                self.channel3.sb_nrx4(v, extra_clock, dmg_corruption_offset);
            }
            0xff1f..=0xff22 => self.channel4.sb(a, v),
            0xff23 => self.channel4.sb_nrx4(v, extra_clock),
            0xff24 => self.reg.nrx0 = v,
            0xff25 => self.reg.nrx1 = v,
            0xff26 => {
                let was_off = !self.reg.get_power();
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
                    // On CGB, length counters are also cleared when APU powers off.
                    // On DMG (monochrome), length counters survive power-off (Pan Docs NR52 footnote 1).
                    if self.term == Term::CGB {
                        self.channel1.lc.n = 0;
                        self.channel2.lc.n = 0;
                        self.channel3.lc.n = 0;
                        self.channel4.lc.n = 0;
                    }
                }
                // Power-on: synchronize the frame-sequencer timer with the DIV
                // counter so that the first FS tick happens at the next falling
                // edge of sdiv bit 12.  Hardware glitch: if bit 12 was already
                // high when the APU powered on, the first DIV-APU event is
                // skipped (SameBoy reference behaviour).
                if was_off && self.reg.get_power() {
                    self.fs.step = 7; // next fs.next() returns 0 (= first length clock)
                    self.timer.n = u32::from(self.sdiv_cache) % 8192;
                    if self.sdiv_cache & 0x1000 != 0 {
                        self.skip_next_fs_tick = true;
                    }
                }
            }
            0xff27..=0xff2f => {}
            0xff30..=0xff3f => {
                // While CH3 is active, writes are redirected to the byte currently
                // being accessed by the wave hardware (CGB: always; DMG: only in the
                // 2-cycle window). Writes outside that window on DMG are ignored.
                // We simplify by always redirecting when the channel is active.
                let is_active = self.channel3.reg.borrow().get_trigger()
                    && self.channel3.reg.borrow().get_dac_power();
                if is_active {
                    self.channel3.waveram[self.channel3.waveidx / 2] = v;
                } else {
                    self.channel3.sb(a, v);
                }
            }
            _ => unreachable!(),
        }
    }
}

fn create_blipbuf(sample_rate: u32) -> BlipBuf {
    let mut blipbuf = BlipBuf::new(sample_rate);
    blipbuf.set_rates(f64::from(CLOCK_FREQUENCY), f64::from(sample_rate));
    blipbuf
}

// Compute the DMG wave-RAM corruption byte offset using SameBoy's timing model.
//
// The wave channel's internal "sample_countdown" timer (2MHz cycles) starts at
// (p1_half + 2) after the first trigger, where p1_half = P1 / 2 = (2048 - freq1).
// It decrements each 2MHz cycle and resets to (P/2 - 1) on each fire.
// Corruption fires only when sample_countdown == 0 at the exact moment of the
// re-trigger write (i.e., the next fire is 1 × 2MHz cycle away).
//
// Returns Some(byte_offset) where byte_offset = ((csi + 1) >> 1) & 0xF,
// or None if sample_countdown ≠ 0.
fn compute_wave_corruption_offset(
    p1_half: u32,    // P1/2 in 2MHz cycles = (2048 - freq_at_first_trigger)
    d1_2mhz: u32,    // 2MHz cycles from first trigger to last NR33 write
    d2_2mhz: u32,    // 2MHz cycles from NR33 write to this re-trigger
    p2_half: u32,    // P2/2 in 2MHz cycles = (2048 - freq_now)
) -> Option<usize> {
    if p1_half == 0 || p2_half == 0 {
        return None;
    }
    let c1 = p1_half - 1; // reset value for phase-1 (P1/2 - 1)
    let c2 = p2_half - 1; // reset value for phase-2 (P2/2 - 1)

    // Initial countdown after trigger: (P1/2 - 1) + 3 = P1/2 + 2
    let mut sc: u32 = c1 + 3;
    let mut csi: u32 = 0;

    // Phase 1: d1_2mhz cycles at period P1
    if sc >= d1_2mhz {
        sc -= d1_2mhz;
    } else {
        let remaining = d1_2mhz - (sc + 1);
        csi += 1;
        let period1 = c1 + 1; // fires every period1 2MHz cycles
        csi = csi.wrapping_add(remaining / period1) % 32;
        sc = c1.saturating_sub(remaining % period1);
    }

    // Phase 2: d2_2mhz cycles at period P2
    if sc >= d2_2mhz {
        sc -= d2_2mhz;
    } else {
        let remaining = d2_2mhz - (sc + 1);
        csi = (csi + 1) % 32;
        let period2 = c2 + 1; // fires every period2 2MHz cycles
        csi = csi.wrapping_add(remaining / period2) % 32;
        sc = c2.saturating_sub(remaining % period2);
    }

    if sc == 0 {
        let byte_offset = ((csi as usize + 1) >> 1) & 0xF;
        Some(byte_offset)
    } else {
        None
    }
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
        Channel::Mixer => CLOCK_FREQUENCY / 512,
    }
}
