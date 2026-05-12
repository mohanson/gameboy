use crate::convention::{Memory, hi, lo};

#[derive(Eq, PartialEq)]
pub enum DmaStatus {
    Gdma, // GDMA
    Hdma, // HDMA
    None, // None
}

pub struct H {
    // These two registers specify the address at which the transfer will read data from. Normally, this should be
    // either in ROM, SRAM or WRAM, thus either in range 0000-7FF0 or A000-DFF0. [Note : this has yet to be tested on
    // Echo RAM, OAM, FEXX, IO and HRAM]. Trying to specify a source address in VRAM will cause garbage to be copied.
    // The four lower bits of this address will be ignored and treated as 0.
    pub src: u16,
    // These two registers specify the address within 8000-9FF0 to which the data will be copied. Only bits 12-4 are
    // respected; others are ignored. The four lower bits of this address will be ignored and treated as 0.
    pub dst: u16,
    // This register specifies the number of 16-byte blocks to be transferred, minus one.
    pub remain: u8,
    pub status: DmaStatus,
}

impl H {
    pub fn power_up() -> Self {
        Self { src: 0x0000, dst: 0x8000, remain: 0x00, status: DmaStatus::None }
    }
}

impl Memory for H {
    fn lb(&self, a: u16) -> u8 {
        match a {
            0xff51 => hi(self.src),
            0xff52 => lo(self.src),
            0xff53 => hi(self.dst),
            0xff54 => lo(self.dst),
            0xff55 => match self.status {
                DmaStatus::Gdma => unreachable!(),
                DmaStatus::Hdma => 0x00 | self.remain,
                DmaStatus::None => 0xff,
            },
            _ => unreachable!(),
        }
    }

    fn sb(&mut self, a: u16, v: u8) {
        match a {
            0xff51 => self.src = (u16::from(v) << 8) | (self.src & 0x00ff),
            0xff52 => self.src = (self.src & 0xff00) | u16::from(v & 0xf0),
            0xff53 => self.dst = 0x8000 | (u16::from(v & 0x1f) << 8) | (self.dst & 0x00ff),
            0xff54 => self.dst = (self.dst & 0xff00) | u16::from(v & 0xf0),
            0xff55 => match self.status {
                DmaStatus::Gdma => unreachable!(),
                DmaStatus::Hdma => match v & 0x80 {
                    0x00 => self.status = DmaStatus::None,
                    0x80 => {}
                    _ => unreachable!(),
                },
                DmaStatus::None => {
                    self.remain = v & 0x7f;
                    match v & 0x80 {
                        0x00 => self.status = DmaStatus::Gdma,
                        0x80 => self.status = DmaStatus::Hdma,
                        _ => unreachable!(),
                    }
                }
            },
            _ => unreachable!(),
        };
    }
}

pub struct O {
    // FF46 — DMA: OAM DMA source address & start. Writing (reg << 8) as the source page triggers a
    // 160-byte copy from (reg << 8)..+(reg<<8|0x9F) into OAM. Valid source range: 0x00–0xDF.
    pub reg: u8,
    // Countdown: loaded to 648 on trigger (2 M-cycle startup delay), active while 1–640.
    pub cnt: u32,
    // Set on write to FF46; consumed on the next advance_clock call to start the countdown.
    pub pending: bool,
}

impl O {
    pub fn power_up() -> Self {
        Self { reg: 0xff, cnt: 0, pending: false }
    }

    pub fn is_active(&self) -> bool {
        self.cnt > 0 && self.cnt <= 640
    }
}

pub struct Dma {
    pub h: H,
    pub o: O,
}

impl Dma {
    pub fn power_up() -> Self {
        Self { h: H::power_up(), o: O::power_up() }
    }
}
