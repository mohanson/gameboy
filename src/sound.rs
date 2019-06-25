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
}

impl Register {
    fn power_up(channel: Channel) -> Self {
        let nrx1 = match channel {
            Channel::Square1 | Channel::Square2 => 0x40,
            _ => 0x00,
        };
        Self {
            channel,
            nrx0: 0x00,
            nrx1,
            nrx2: 0x00,
            nrx3: 0x00,
            nrx4: 0x00,
        }
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
            self.n = if self.reg.borrow().channel == Channel::Wave {
                1 << 8
            } else {
                1 << 6
            };
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
//
// NRX2 ---- VVVV APPP Starting volume, Envelope add mode, period
struct VolumeEnvelope {
    reg: Rc<RefCell<Register>>,
    volume: u8,
    d: u8,
}

impl VolumeEnvelope {
    fn power_up(reg: Rc<RefCell<Register>>) -> Self {
        Self {
            reg,
            volume: 0x00,
            d: 0x00,
        }
    }

    fn reload(&mut self) {
        self.d = self.reg.borrow().get_period();
        self.volume = self.reg.borrow().get_starting_volume();
    }

    fn next(&mut self) {
        if self.d == 0 {
            return;
        }
        if self.d == 1 {
            self.d = self.reg.borrow().get_period();
            // If this new volume within the 0 to 15 range, the volume is updated
            let v = if self.reg.borrow().get_envelope_add_mode() {
                self.volume.wrapping_add(1)
            } else {
                self.volume.wrapping_sub(1)
            };
            if v <= 15 {
                self.volume = v;
            }
            return;
        }
        self.d -= 1;
    }
}

const WAVE_PATTERN: [[i32; 8]; 4] = [
    [-1, -1, -1, -1, 1, -1, -1, -1],
    [-1, -1, -1, -1, 1, 1, -1, -1],
    [-1, -1, 1, 1, 1, 1, -1, -1],
    [1, 1, 1, 1, -1, -1, 1, 1],
];
const CLOCKS_PER_SECOND: u32 = 1 << 22;
const OUTPUT_SAMPLE_COUNT: usize = 2000; // this should be less than blip_buf::MAX_FRAME

struct SquareChannel {
    reg: Rc<RefCell<Register>>,
    lc: LengthCounter,
    ve: VolumeEnvelope,
    phase: u8,
    period: u32,
    last_amp: i32,
    delay: u32,
    has_sweep: bool,
    sweep_frequency: u16,
    sweep_delay: u8,
    sweep_period: u8,
    sweep_shift: u8,
    sweep_frequency_increase: bool,
    blip: BlipBuf,
}

impl SquareChannel {
    fn new(blip: BlipBuf, with_sweep: bool) -> SquareChannel {
        let reg = if with_sweep {
            Rc::new(RefCell::new(Register::power_up(Channel::Square1)))
        } else {
            Rc::new(RefCell::new(Register::power_up(Channel::Square2)))
        };
        SquareChannel {
            reg: reg.clone(),
            lc: LengthCounter::power_up(reg.clone()),
            ve: VolumeEnvelope::power_up(reg.clone()),
            phase: 1,
            period: 2048,
            last_amp: 0,
            delay: 0,
            has_sweep: with_sweep,
            sweep_frequency: 0,
            sweep_delay: 0,
            sweep_period: 0,
            sweep_shift: 0,
            sweep_frequency_increase: false,
            blip,
        }
    }

    fn wb(&mut self, a: u16, v: u8) {
        match a {
            0xff10 => {
                self.reg.borrow_mut().nrx0 = v;
                self.sweep_period = (v >> 4) & 0x7;
                self.sweep_shift = v & 0x7;
                self.sweep_frequency_increase = v & 0x8 == 0x8;
            }
            0xff11 | 0xff16 => {
                self.reg.borrow_mut().nrx1 = v;
                self.lc.n = self.reg.borrow().get_length_load();
            }
            0xff12 | 0xff17 => self.reg.borrow_mut().nrx2 = v,
            0xff13 | 0xff18 => {
                self.reg.borrow_mut().nrx3 = v;
                self.calculate_period();
            }
            0xff14 | 0xff19 => {
                self.reg.borrow_mut().nrx4 = v;
                self.calculate_period();

                if self.reg.borrow().get_trigger() {
                    self.lc.reload();
                    self.ve.reload();

                    self.sweep_frequency = self.reg.borrow().get_frequency();
                    if self.has_sweep && self.sweep_period > 0 && self.sweep_shift > 0 {
                        self.sweep_delay = 1;
                        self.step_sweep();
                    }
                }
            }
            _ => {}
        }
    }

    fn calculate_period(&mut self) {
        if self.reg.borrow().get_frequency() > 2048 {
            self.period = 0;
        } else {
            self.period = (2048 - u32::from(self.reg.borrow().get_frequency())) * 4;
        }
    }

    // This assumes no volume or sweep adjustments need to be done in the meantime
    fn run(&mut self, start_time: u32, end_time: u32) {
        if !self.reg.borrow().get_trigger() || self.period == 0 || self.ve.volume == 0 {
            if self.last_amp != 0 {
                self.blip.add_delta(start_time, -self.last_amp);
                self.last_amp = 0;
                self.delay = 0;
            }
        } else {
            let mut time = start_time + self.delay;
            let pattern = WAVE_PATTERN[self.reg.borrow().get_duty() as usize];
            let vol = i32::from(self.ve.volume);

            while time < end_time {
                let amp = vol * pattern[self.phase as usize];
                if amp != self.last_amp {
                    self.blip.add_delta(time, amp - self.last_amp);
                    self.last_amp = amp;
                }
                time += self.period;
                self.phase = (self.phase + 1) % 8;
            }

            // next time, we have to wait an additional delay timesteps
            self.delay = time - end_time;
        }
    }

    fn step_sweep(&mut self) {
        if !self.has_sweep || self.sweep_period == 0 {
            return;
        }

        if self.sweep_delay > 1 {
            self.sweep_delay -= 1;
        } else {
            self.sweep_delay = self.sweep_period;
            self.reg.borrow_mut().set_frequency(self.sweep_frequency);
            if self.reg.borrow().get_frequency() == 2048 {
                self.reg.borrow_mut().set_trigger(false);
            }
            self.calculate_period();

            let offset = self.sweep_frequency >> self.sweep_shift;

            if self.sweep_frequency_increase {
                // F ~ (2048 - f)
                // Increase in frequency means subtracting the offset
                if self.sweep_frequency <= offset {
                    self.sweep_frequency = 0;
                } else {
                    self.sweep_frequency -= offset;
                }
            } else if self.sweep_frequency >= 2048 - offset {
                self.sweep_frequency = 2048;
            } else {
                self.sweep_frequency += offset;
            }
        }
    }
}

struct WaveChannel {
    reg: Rc<RefCell<Register>>,
    lc: LengthCounter,
    period: u32,
    last_amp: i32,
    delay: u32,
    waveram: [u8; 32],
    current_wave: u8,
    blip: BlipBuf,
}

impl WaveChannel {
    fn new(blip: BlipBuf) -> WaveChannel {
        let reg = Rc::new(RefCell::new(Register::power_up(Channel::Wave)));
        WaveChannel {
            reg: reg.clone(),
            lc: LengthCounter::power_up(reg.clone()),
            period: 2048,
            last_amp: 0,
            delay: 0,
            waveram: [0; 32],
            current_wave: 0,
            blip,
        }
    }

    fn wb(&mut self, a: u16, v: u8) {
        match a {
            0xff1a => {
                self.reg.borrow_mut().nrx0 = v;
            }
            0xff1b => {
                self.reg.borrow_mut().nrx1 = v;
                self.lc.n = self.reg.borrow().get_length_load();
            }
            0xff1c => {
                self.reg.borrow_mut().nrx2 = v;
            }
            0xff1d => {
                self.reg.borrow_mut().nrx3 = v;
                self.calculate_period();
            }
            0xff1e => {
                self.reg.borrow_mut().nrx4 = v;
                self.calculate_period();
                if self.reg.borrow().get_trigger() && self.reg.borrow().get_dac_power() {
                    self.lc.reload();
                    self.current_wave = 0;
                    self.delay = 0;
                }
            }
            0xff30...0xff3f => {
                self.waveram[(a as usize - 0xFF30) / 2] = v >> 4;
                self.waveram[(a as usize - 0xFF30) / 2 + 1] = v & 0xF;
            }
            _ => {}
        }
    }

    fn calculate_period(&mut self) {
        if self.reg.borrow().get_frequency() > 2048 {
            self.period = 0;
        } else {
            self.period = (2048 - u32::from(self.reg.borrow().get_frequency())) * 2;
        }
    }

    fn run(&mut self, start_time: u32, end_time: u32) {
        if !self.reg.borrow().get_trigger() || self.period == 0 {
            if self.last_amp != 0 {
                self.blip.add_delta(start_time, -self.last_amp);
                self.last_amp = 0;
                self.delay = 0;
            }
        } else {
            let mut time = start_time + self.delay;

            let volshift = match self.reg.borrow().get_volume_code() {
                0 => 4,
                1 => 0,
                2 => 1,
                3 => 2,
                _ => unreachable!(),
            };

            while time < end_time {
                let sample = self.waveram[self.current_wave as usize];

                let amp = i32::from(sample >> volshift);

                if amp != self.last_amp {
                    self.blip.add_delta(time, amp - self.last_amp);
                    self.last_amp = amp;
                }

                time += self.period;
                self.current_wave = (self.current_wave + 1) % 32;
            }

            // next time, we have to wait an additional delay timesteps
            self.delay = time - end_time;
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
        let s = if self.reg.borrow().get_width_mode_of_lfsr() {
            0x06
        } else {
            0x0e
        };
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

struct NoiseChannel {
    reg: Rc<RefCell<Register>>,
    lc: LengthCounter,
    ve: VolumeEnvelope,
    lfsr: Lfsr,
    period: u32,
    delay: u32,
    last_amp: i32,
    blip: BlipBuf,
}

impl NoiseChannel {
    fn new(blip: BlipBuf) -> NoiseChannel {
        let reg = Rc::new(RefCell::new(Register::power_up(Channel::Noise)));
        NoiseChannel {
            reg: reg.clone(),
            lc: LengthCounter::power_up(reg.clone()),
            ve: VolumeEnvelope::power_up(reg.clone()),
            lfsr: Lfsr::power_up(reg.clone()),
            period: 2048,
            delay: 0,
            last_amp: 0,
            blip,
        }
    }

    fn wb(&mut self, a: u16, v: u8) {
        match a {
            0xff20 => {
                self.reg.borrow_mut().nrx1 = v;
                self.lc.n = self.reg.borrow().get_length_load();
            }
            0xff21 => self.reg.borrow_mut().nrx2 = v,
            0xff22 => {
                self.reg.borrow_mut().nrx3 = v;
                let freq_div = match self.reg.borrow().get_dividor_code() {
                    0 => 8,
                    n => (u32::from(n) + 1) * 16,
                };
                self.period = freq_div << self.reg.borrow().get_clock_shift();
            }
            0xff23 => {
                self.reg.borrow_mut().nrx4 = v;
                if self.reg.borrow().get_trigger() {
                    self.lc.reload();
                    self.ve.reload();
                    self.lfsr.reload();
                    self.delay = 0;
                }
            }
            _ => {}
        }
    }

    fn run(&mut self, start_time: u32, end_time: u32) {
        if !self.reg.borrow().get_trigger() || self.ve.volume == 0 {
            if self.last_amp != 0 {
                self.blip.add_delta(start_time, -self.last_amp);
                self.last_amp = 0;
                self.delay = 0;
            }
        } else {
            let mut time = start_time + self.delay;
            while time < end_time {
                let amp = if self.lfsr.next() {
                    i32::from(self.ve.volume)
                } else {
                    -i32::from(self.ve.volume)
                };
                if self.last_amp != amp {
                    self.blip.add_delta(time, amp - self.last_amp);
                    self.last_amp = amp;
                }

                time += self.period;
            }
            self.delay = time - end_time;
        }
    }
}

pub struct Sound {
    on: bool,
    registerdata: [u8; 0x17],
    time: u32,
    prev_time: u32,
    next_time: u32,
    time_divider: u8,
    output_period: u32,
    channel1: SquareChannel,
    channel2: SquareChannel,
    channel3: WaveChannel,
    channel4: NoiseChannel,
    volume_left: u8,
    volume_right: u8,
    need_sync: bool,
    pub buffer: Arc<Mutex<Vec<(f32, f32)>>>,
    sample_rate: u32,
}

impl Sound {
    pub fn new(sample_rate: u32) -> Sound {
        let blipbuf1 = create_blipbuf(sample_rate);
        let blipbuf2 = create_blipbuf(sample_rate);
        let blipbuf3 = create_blipbuf(sample_rate);
        let blipbuf4 = create_blipbuf(sample_rate);

        let output_period = (OUTPUT_SAMPLE_COUNT as u64 * u64::from(CLOCKS_PER_SECOND)) / u64::from(sample_rate);

        Sound {
            on: false,
            registerdata: [0; 0x17],
            time: 0,
            prev_time: 0,
            next_time: CLOCKS_PER_SECOND / 256,
            time_divider: 0,
            output_period: output_period as u32,
            channel1: SquareChannel::new(blipbuf1, true),
            channel2: SquareChannel::new(blipbuf2, false),
            channel3: WaveChannel::new(blipbuf3),
            channel4: NoiseChannel::new(blipbuf4),
            volume_left: 7,
            volume_right: 7,
            need_sync: false,
            buffer: Arc::new(Mutex::new(Vec::new())),
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

    fn underflowed(&self) -> bool {
        (*self.buffer.lock().unwrap()).is_empty()
    }

    pub fn rb(&self, a: u16) -> u8 {
        // self.run();
        match a {
            0xFF10...0xFF25 => self.registerdata[a as usize - 0xFF10],
            0xFF26 => {
                (self.registerdata[a as usize - 0xFF10] & 0xF0)
                    | (if self.channel1.reg.borrow().get_trigger() { 1 } else { 0 })
                    | (if self.channel2.reg.borrow().get_trigger() { 2 } else { 0 })
                    | (if self.channel3.reg.borrow().get_trigger() && self.channel3.reg.borrow().get_dac_power() {
                        4
                    } else {
                        0
                    })
                    | (if self.channel4.reg.borrow().get_trigger() { 8 } else { 0 })
            }
            0xFF30...0xFF3F => {
                (self.channel3.waveram[(a as usize - 0xFF30) / 2] << 4)
                    | self.channel3.waveram[(a as usize - 0xFF30) / 2 + 1]
            }
            _ => 0,
        }
    }

    pub fn wb(&mut self, a: u16, v: u8) {
        if a != 0xFF26 && !self.on {
            return;
        }
        self.run();
        if a >= 0xFF10 && a <= 0xFF26 {
            self.registerdata[a as usize - 0xFF10] = v;
        }
        match a {
            0xFF10...0xFF14 => self.channel1.wb(a, v),
            0xFF16...0xFF19 => self.channel2.wb(a, v),
            0xFF1A...0xFF1E => self.channel3.wb(a, v),
            0xFF20...0xFF23 => self.channel4.wb(a, v),
            0xFF24 => {
                self.volume_left = v & 0x7;
                self.volume_right = (v >> 4) & 0x7;
            }
            0xFF26 => self.on = v & 0x80 == 0x80,
            0xFF30...0xFF3F => self.channel3.wb(a, v),
            _ => (),
        }
    }

    pub fn do_cycle(&mut self, cycles: u32) {
        if !self.on {
            return;
        }

        self.time += cycles;

        if self.time >= self.output_period {
            self.do_output();
        }
    }

    pub fn sync(&mut self) {
        self.need_sync = true;
    }

    fn do_output(&mut self) {
        self.run();
        debug_assert!(self.time == self.prev_time);
        self.channel1.blip.end_frame(self.time);
        self.channel2.blip.end_frame(self.time);
        self.channel3.blip.end_frame(self.time);
        self.channel4.blip.end_frame(self.time);
        self.next_time -= self.time;
        self.time = 0;
        self.prev_time = 0;

        if !self.need_sync || self.underflowed() {
            self.need_sync = false;
            self.mix_buffers();
        } else {
            // Prevent the BlipBuf's from filling up and triggering an assertion
            self.clear_buffers();
        }
    }

    fn run(&mut self) {
        while self.next_time <= self.time {
            self.channel1.run(self.prev_time, self.next_time);
            self.channel2.run(self.prev_time, self.next_time);
            self.channel3.run(self.prev_time, self.next_time);
            self.channel4.run(self.prev_time, self.next_time);

            self.channel1.lc.next();
            self.channel2.lc.next();
            self.channel3.lc.next();
            self.channel4.lc.next();

            if self.time_divider == 0 {
                self.channel1.ve.next();
                self.channel2.ve.next();
                self.channel4.ve.next();
            } else if self.time_divider & 1 == 1 {
                self.channel1.step_sweep();
            }

            self.time_divider = (self.time_divider + 1) % 4;
            self.prev_time = self.next_time;
            self.next_time += CLOCKS_PER_SECOND / 256;
        }

        if self.prev_time != self.time {
            self.channel1.run(self.prev_time, self.time);
            self.channel2.run(self.prev_time, self.time);
            self.channel3.run(self.prev_time, self.time);
            self.channel4.run(self.prev_time, self.time);

            self.prev_time = self.time;
        }
    }

    fn mix_buffers(&mut self) {
        let sample_count = self.channel1.blip.samples_avail() as usize;
        debug_assert!(sample_count == self.channel2.blip.samples_avail() as usize);
        debug_assert!(sample_count == self.channel3.blip.samples_avail() as usize);
        debug_assert!(sample_count == self.channel4.blip.samples_avail() as usize);

        let mut outputted = 0;

        let left_vol = (f32::from(self.volume_left) / 7.0) * (1.0 / 15.0) * 0.25;
        let right_vol = (f32::from(self.volume_right) / 7.0) * (1.0 / 15.0) * 0.25;

        while outputted < sample_count {
            let buf_left = &mut [0f32; OUTPUT_SAMPLE_COUNT + 10];
            let buf_right = &mut [0f32; OUTPUT_SAMPLE_COUNT + 10];
            let buf = &mut [0i16; OUTPUT_SAMPLE_COUNT + 10];

            let count1 = self.channel1.blip.read_samples(buf, false);
            for (i, v) in buf[..count1].iter().enumerate() {
                if self.registerdata[0x15] & 0x01 == 0x01 {
                    buf_left[i] += f32::from(*v) * left_vol;
                }
                if self.registerdata[0x15] & 0x10 == 0x10 {
                    buf_right[i] += f32::from(*v) * right_vol;
                }
            }

            let count2 = self.channel2.blip.read_samples(buf, false);
            for (i, v) in buf[..count2].iter().enumerate() {
                if self.registerdata[0x15] & 0x02 == 0x02 {
                    buf_left[i] += f32::from(*v) * left_vol;
                }
                if self.registerdata[0x15] & 0x20 == 0x20 {
                    buf_right[i] += f32::from(*v) * right_vol;
                }
            }

            let count3 = self.channel3.blip.read_samples(buf, false);
            for (i, v) in buf[..count3].iter().enumerate() {
                if self.registerdata[0x15] & 0x04 == 0x04 {
                    buf_left[i] += f32::from(*v) * left_vol;
                }
                if self.registerdata[0x15] & 0x40 == 0x40 {
                    buf_right[i] += f32::from(*v) * right_vol;
                }
            }

            let count4 = self.channel4.blip.read_samples(buf, false);
            for (i, v) in buf[..count4].iter().enumerate() {
                if self.registerdata[0x15] & 0x08 == 0x08 {
                    buf_left[i] += f32::from(*v) * left_vol;
                }
                if self.registerdata[0x15] & 0x80 == 0x80 {
                    buf_right[i] += f32::from(*v) * right_vol;
                }
            }

            debug_assert!(count1 == count2);
            debug_assert!(count1 == count3);
            debug_assert!(count1 == count4);

            self.play(&buf_left[..count1], &buf_right[..count1]);

            outputted += count1;
        }
    }

    fn clear_buffers(&mut self) {
        self.channel1.blip.clear();
        self.channel2.blip.clear();
        self.channel3.blip.clear();
        self.channel4.blip.clear();
    }
}

fn create_blipbuf(sample_rate: u32) -> BlipBuf {
    let mut blipbuf = BlipBuf::new(sample_rate);
    blipbuf.set_rates(f64::from(CLOCKS_PER_SECOND), f64::from(sample_rate));
    blipbuf
}
