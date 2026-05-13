// A memory management unit (MMU), sometimes called paged memory management unit (PMMU), is a computer hardware unit
// having all memory references passed through itself, primarily performing the translation of virtual memory addresses
// to physical addresses.
use crate::apu::Apu;
use crate::cartridge::Cartridge;
use crate::convention::{Global, Memory, Term, Ticker};
use crate::dma::{Dma, DmaStatus};
use crate::gpu::Gpu;
use crate::interrupt::Interrupt;
use crate::joypad::Joypad;
use crate::rng;
use crate::serial::Serial;
use crate::timer::Timer;
use std::cell::RefCell;
use std::rc::Rc;

pub struct Mmu {
    pub glo: Rc<RefCell<Global>>,
    pub apu: Apu,
    pub dma: Dma,
    pub gpu: Gpu,
    pub hram: [u8; 0x7f],
    pub intr: Interrupt,
    pub joypad: Joypad,
    pub rom: Cartridge,
    pub serial: Serial,
    pub spd: u8,
    pub ticker: Timer,
    pub wram: [u8; 0x8000],
    pub wram_bank: usize,
}

impl Mmu {
    pub fn power_up(glo: Rc<RefCell<Global>>, rom: Cartridge) -> Self {
        let mut r = Self {
            glo: glo.clone(),
            apu: Apu::power_up(glo.clone(), 48000),
            dma: Dma::power_up(),
            gpu: Gpu::power_up(glo.clone()),
            hram: [0x00; 0x7f],
            intr: Interrupt::power_up(glo.clone()),
            joypad: Joypad::power_up(glo.clone()),
            rom,
            serial: Serial::power_up(glo.clone()),
            spd: 0x00,
            ticker: Timer::power_up(glo.clone()),
            wram: [0x00; 0x8000],
            wram_bank: 0x01,
        };
        r.sb(0xff26, 0xf1);
        r.sb(0xff10, 0x80);
        r.sb(0xff11, 0xbf);
        r.sb(0xff12, 0xf3);
        r.sb(0xff13, 0xff);
        r.sb(0xff14, 0xbf);
        r.sb(0xff16, 0x3f);
        r.sb(0xff17, 0x00);
        r.sb(0xff18, 0xff);
        r.sb(0xff19, 0x3f);
        r.sb(0xff1a, 0x7f);
        r.sb(0xff1b, 0xff);
        r.sb(0xff1c, 0x9f);
        r.sb(0xff1d, 0xff);
        r.sb(0xff1e, 0xbf);
        r.sb(0xff20, 0xff);
        r.sb(0xff21, 0x00);
        r.sb(0xff22, 0x00);
        r.sb(0xff23, 0x3f);
        r.sb(0xff24, 0x77);
        r.sb(0xff25, 0xf3);
        r.sb(0xff40, 0x91);
        r.sb(0xff41, 0x85);
        r.sb(0xff42, 0x00);
        r.sb(0xff43, 0x00);
        r.sb(0xff44, 0x00);
        r.sb(0xff45, 0x00);
        r.sb(0xff47, 0xfc);
        r.sb(0xff48, rng::u8());
        r.sb(0xff49, rng::u8());
        r.sb(0xff4a, 0x00);
        r.sb(0xff4b, 0x00);
        r
    }
}

impl Memory for Mmu {
    fn lb(&self, a: u16) -> u8 {
        match a {
            0x0000..=0x7fff => self.rom.lb(a),
            0x8000..=0x9fff => self.gpu.lb(a),
            0xa000..=0xbfff => self.rom.lb(a),
            0xc000..=0xcfff => self.wram[a as usize - 0xc000],
            0xd000..=0xdfff => self.wram[a as usize - 0xd000 + 0x1000 * self.wram_bank],
            0xe000..=0xfdff => self.lb(a - 0x2000),
            0xfe00..=0xfe9f => self.lb_odma(a),
            0xfea0..=0xfeff => 0xff,
            0xff00 => self.joypad.lb(a),
            0xff01..=0xff02 => self.serial.lb(a),
            0xff04..=0xff07 => self.ticker.lb(a),
            0xff0f => self.intr.lb(0xff0f),
            0xff10..=0xff3f => self.apu.lb(a),
            0xff40..=0xff45 => self.gpu.lb(a),
            0xff46 => self.dma.o.lb(0xff46),
            0xff47..=0xff4b => self.gpu.lb(a),
            0xff4c..=0xff70 => match self.glo.borrow().term {
                Term::DMG => 0xff,
                Term::CGB => match a {
                    0xff4d => 0x7e | self.spd,
                    0xff4f => self.gpu.lb(a),
                    0xff51..=0xff55 => self.dma.h.lb(a),
                    0xff68..=0xff6b => self.gpu.lb(a),
                    0xff70 => self.wram_bank as u8,
                    _ => 0xff,
                },
            },
            0xff80..=0xfffe => self.hram[a as usize - 0xff80],
            0xffff => self.intr.lb(0xffff),
            _ => 0xff,
        }
    }

    fn sb(&mut self, a: u16, v: u8) {
        match a {
            0x0000..=0x7fff => self.rom.sb(a, v),
            0x8000..=0x9fff => self.gpu.sb(a, v),
            0xa000..=0xbfff => self.rom.sb(a, v),
            0xc000..=0xcfff => self.wram[a as usize - 0xc000] = v,
            0xd000..=0xdfff => self.wram[a as usize - 0xd000 + 0x1000 * self.wram_bank] = v,
            0xe000..=0xfdff => self.sb(a - 0x2000, v),
            0xfe00..=0xfe9f => {
                // CPU writes to OAM are blocked while OAM DMA is active (bus conflict).
                if self.dma.o.is_active() {
                    return;
                }
                self.gpu.sb(a, v);
            }
            0xfea0..=0xfeff => {}
            0xff00 => self.joypad.sb(a, v),
            0xff01..=0xff02 => self.serial.sb(a, v),
            0xff04..=0xff07 => self.ticker.sb(a, v),
            0xff0f => self.intr.sb(0xff0f, v),
            0xff10..=0xff3f => {
                self.apu.sdiv_cache = self.glo.borrow().sdiv;
                self.apu.sb(a, v);
            }
            0xff40..=0xff45 => self.gpu.sb(a, v),
            0xff46 => self.dma.o.sb(0xff46, v),
            0xff47..=0xff4b => self.gpu.sb(a, v),
            0xff4c..=0xff70 => match self.glo.borrow().term {
                Term::DMG => {}
                Term::CGB => match a {
                    0xff4d => self.spd = (self.spd & 0x80) | (v & 0x01),
                    0xff4f => self.gpu.sb(a, v),
                    0xff51..=0xff55 => self.dma.h.sb(a, v),
                    0xff68..=0xff6b => self.gpu.sb(a, v),
                    0xff70 => self.wram_bank = (v as usize & 0x7).max(1),
                    _ => {}
                },
            },
            0xff80..=0xfffe => self.hram[a as usize - 0xff80] = v,
            0xffff => self.intr.sb(0xffff, v),
            _ => {}
        }
    }
}

impl Ticker for Mmu {
    fn tick(&mut self, cycles: u16) {
        self.next(cycles);
        self.odma(cycles);
        let cycles = self.hdma();
        self.gpu.h_blank = false;
        if cycles != 0 {
            return;
        }
        self.next(cycles);
        self.gpu.h_blank = false;
    }
}

impl Mmu {
    fn next(&mut self, cycles: u16) {
        self.ticker.tick(cycles);
        self.serial.tick(cycles);
        let cycles = if self.spd & 0x80 != 0 { cycles / 2 } else { cycles };
        self.gpu.next(cycles as u32);
        self.apu.next(cycles as u32);
    }

    fn hdma(&mut self) -> u16 {
        match self.dma.h.status {
            DmaStatus::None => 0,
            DmaStatus::Gdma => {
                let len = u16::from(self.dma.h.remain) + 1;
                for _ in 0..len {
                    self.bdma();
                }
                len * 8
            }
            DmaStatus::Hdma => {
                if !self.gpu.h_blank {
                    return 0;
                }
                self.bdma();
                8
            }
        }
    }

    fn bdma(&mut self) {
        for _ in 0x00..0x10 {
            let b = self.lb(self.dma.h.src);
            self.gpu.sb(self.dma.h.dst, b);
            self.dma.h.src = self.dma.h.src.wrapping_add(1);
            self.dma.h.dst = self.dma.h.dst.wrapping_add(1);
        }
        self.dma.h.remain = self.dma.h.remain.wrapping_sub(1);
        if self.dma.h.remain == 0xff {
            self.dma.h.status = DmaStatus::None;
        }
    }

    fn odma(&mut self, cycles: u16) {
        let since = self.dma.o.cnt;
        if self.dma.o.sig != 0x00 {
            self.dma.o.sig = 0x00;
            if self.dma.o.cnt <= 639 {
                // 2 M-cycle startup delay (fresh start or restart mid-copy).
                self.dma.o.cnt = 648;
            }
        }
        if self.dma.o.cnt != 0 {
            self.dma.o.cnt = self.dma.o.cnt.saturating_sub(cycles);
        }
        if since == 0 || since.min(640) <= self.dma.o.cnt {
            return;
        }
        let src_byte = (640u16.saturating_sub(since.min(640)) + 3) / 4;
        let end_byte = (640u16.saturating_sub(self.dma.o.cnt) + 3) / 4;
        let src_page = (self.dma.o.reg as u16) << 8;
        for i in src_byte..end_byte.min(160) {
            let src = src_page | i as u16;
            let src = if src <= 0xdfff { src } else { src - 0x2000 };
            let b = match src {
                0x0000..=0x7fff => self.rom.lb(src),
                0x8000..=0x9fff => self.gpu.lb(src),
                0xa000..=0xbfff => self.rom.lb(src),
                0xc000..=0xcfff => self.wram[src as usize - 0xc000],
                0xd000..=0xdfff => self.wram[src as usize - 0xd000 + 0x1000 * self.wram_bank],
                _ => 0xff,
            };
            self.gpu.sb(0xfe00 + i as u16, b);
        }
    }
}

impl Mmu {
    pub fn notify_spd(&mut self) {
        self.spd ^= 0x80;
        self.spd &= 0xfe;
    }

    pub fn lb_odma(&self, a: u16) -> u8 {
        match a {
            0xfe00..=0xfe9f => {
                if self.dma.o.is_active() {
                    0xff
                } else {
                    self.gpu.lb(a)
                }
            }
            _ => unreachable!(),
        }
    }

    /// Called by the CPU when its IDU (Increment/Decrement Unit) operates with
    /// `addr` as the current register-pair value. The MMU applies any address-
    /// range-specific bus side effects (e.g. OAM corruption on DMG) internally.
    pub fn notify_idu(&mut self, addr: u16) {
        if addr >> 8 == 0xfe {
            self.oam_write_corrupt();
        }
    }

    /// Called by the CPU when a bus read occurs simultaneously with an IDU
    /// operation (Read-During-IDU), e.g. LDI/LDD or the first byte of POP.
    pub fn notify_rdi(&mut self, addr: u16) {
        if addr >> 8 == 0xfe {
            self.oam_rdi_corrupt();
        }
    }

    /// Called by the CPU when a sequential (non-IDU) bus read occurs, e.g.
    /// the second byte of a POP instruction.
    pub fn notify_seq(&mut self, addr: u16) {
        if addr >> 8 == 0xfe {
            self.oam_read_corrupt();
        }
    }

    fn oam_write_corrupt(&mut self) {
        if self.glo.borrow().term == Term::DMG {
            self.gpu.oam_write_corrupt();
        }
    }

    fn oam_read_corrupt(&mut self) {
        if self.glo.borrow().term == Term::DMG {
            self.gpu.oam_read_corrupt();
        }
    }

    fn oam_rdi_corrupt(&mut self) {
        if self.glo.borrow().term == Term::DMG {
            self.gpu.oam_rdi_corrupt();
        }
    }
}
