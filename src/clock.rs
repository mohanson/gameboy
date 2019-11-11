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
