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
    pub reg: u8,
    pub cnt: u32,
    pub sig: u8,
}

impl Memory for O {
    fn lb(&self, a: u16) -> u8 {
        assert_eq!(a, 0xff46);
        self.reg
    }

    fn sb(&mut self, a: u16, v: u8) {
        assert_eq!(a, 0xff46);
        self.reg = v;
        self.sig = 0x01;
    }
}

impl O {
    pub fn power_up() -> Self {
        Self { reg: 0xff, cnt: 0x00000000, sig: 0x00 }
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
