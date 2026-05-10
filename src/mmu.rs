// A memory management unit (MMU), sometimes called paged memory management unit (PMMU), is a computer hardware unit
// having all memory references passed through itself, primarily performing the translation of virtual memory addresses
// to physical addresses.
use crate::apu::Apu;
use crate::cartridge::Cartridge;
use crate::convention::{Global, Hollow, Memory, Term, Ticker};
use crate::dma::Dma;
use crate::gpu::{Gpu, Hdma, HdmaMode};
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
    pub cartridge: Cartridge,
    pub dma: Dma,
    pub gpu: Gpu,
    pub hdma: Hdma,
    pub hram: [u8; 0x7f],
    pub intr: Rc<RefCell<Interrupt>>,
    pub joypad: Joypad,
    pub serial: Serial,
    pub term: Term,
    pub timer: Timer,
    pub wram: [u8; 0x8000],
    pub wram_bank: usize,
    pub speed: u8,
    pub speed_switch: bool,
}

impl Mmu {
    pub fn power_up(glo: Rc<RefCell<Global>>, rom: Cartridge) -> Self {
        let term = glo.borrow().term;
        let intr = Rc::new(RefCell::new(Interrupt::power_up(glo.clone())));
        let mut r = Self {
            glo: glo.clone(),
            apu: Apu::power_up(48000, term),
            cartridge: rom,
            dma: Dma::power_up(Rc::new(RefCell::new(Hollow::power_up()))),
            gpu: Gpu::power_up(term, intr.clone()),
            hdma: Hdma::power_up(),
            hram: [0x00; 0x7f],
            intr: intr.clone(),
            joypad: Joypad::power_up(glo.clone()),
            serial: Serial::power_up(glo.clone()),
            term,
            timer: Timer::power_up(glo.clone()),
            wram: [0x00; 0x8000],
            wram_bank: 0x01,
            speed: 1,
            speed_switch: false,
        };
        r.sb(0xff26, 0xf1); // Must be first: enables APU power so subsequent channel writes are not blocked.
        r.sb(0xff10, 0x80);
        r.sb(0xff11, 0xbf);
        r.sb(0xff12, 0xf3);
        r.sb(0xff13, 0xff);
        r.sb(0xff14, 0xbf);
        r.sb(0xff16, 0x3f);
        r.sb(0xff17, 0x00);
        r.sb(0xff18, 0xff);
        r.sb(0xff19, 0x3f); // No trigger (bit 7 = 0): Channel 2 is inactive post-boot.
        r.sb(0xff1a, 0x7f);
        r.sb(0xff1b, 0xff);
        r.sb(0xff1c, 0x9f);
        r.sb(0xff1d, 0xff);
        r.sb(0xff1e, 0xbf);
        r.sb(0xff20, 0xff);
        r.sb(0xff21, 0x00);
        r.sb(0xff22, 0x00);
        r.sb(0xff23, 0x3f); // No trigger (bit 7 = 0): Channel 4 is inactive post-boot.
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

impl Mmu {
    fn video_cycles(&self, cycles: u32) -> u32 {
        if self.speed == 2 { cycles / 2 } else { cycles }
    }

    pub fn try_switch_speed(&mut self) -> bool {
        if self.term != Term::CGB || !self.speed_switch {
            return false;
        }
        self.speed = if self.speed == 1 { 2 } else { 1 };
        self.speed_switch = false;
        true
    }

    /// Called once per CPU instruction after all per-M-cycle ticks have already advanced
    /// timer/GPU/APU. Runs HDMA and resets the h_blank edge signal.
    pub fn next(&mut self) -> u32 {
        let hdma_cycles = self.run_dma();
        self.gpu.h_blank = false;
        if hdma_cycles > 0 {
            self.timer.tick(hdma_cycles);
            let video_cycles = self.video_cycles(hdma_cycles);
            self.gpu.next(video_cycles);
            self.apu.next(video_cycles);
            self.gpu.h_blank = false;
        }
        hdma_cycles
    }

    fn advance_clock(&mut self, cycles: u32) {
        self.timer.tick(cycles);
        self.serial.tick(cycles);
        let video_cycles = self.video_cycles(cycles);
        self.gpu.next(video_cycles);
        self.apu.next(video_cycles);
        self.dma.o.advance_counter(cycles);
    }

    pub fn lb_odma(&self, a: u16) -> u8 {
        match a {
            0xfe00..=0xfe9f => {
                let cnt = self.dma.o.cnt.borrow().clone();
                if cnt > 0 && cnt <= 640 {
                    return 0xff;
                }
                self.gpu.lb(a)
            }
            _ => unreachable!(),
        }
    }

    /// Trigger the DMG OAM write-corruption bug.
    /// Call this BEFORE the internal() cycle of an INC/DEC rr instruction
    /// when the old register value is in $FE00–$FEFF and we are on a DMG.
    pub fn trigger_oam_write_bug(&mut self) {
        if self.term == Term::DMG {
            self.gpu.oam_write_corrupt();
        }
    }

    /// Trigger the DMG OAM read-corruption bug.
    /// Call this when a CPU memory read lands in $FE00–$FEFF during mode 2.
    pub fn trigger_oam_read_bug(&mut self) {
        if self.term == Term::DMG {
            self.gpu.oam_read_corrupt();
        }
    }

    /// Trigger the DMG OAM "Read During Increase/Decrease" corruption bug.
    /// Call this for POP-type instructions where the bus read and IDU happen simultaneously.
    pub fn trigger_oam_rdi_bug(&mut self) {
        if self.term == Term::DMG {
            self.gpu.oam_rdi_corrupt();
        }
    }
}

impl Memory for Mmu {
    fn lb(&self, a: u16) -> u8 {
        match a {
            0x0000..=0x7fff => self.cartridge.lb(a),
            0x8000..=0x9fff => self.gpu.lb(a),
            0xa000..=0xbfff => self.cartridge.lb(a),
            0xc000..=0xcfff => self.wram[a as usize - 0xc000],
            0xd000..=0xdfff => self.wram[a as usize - 0xd000 + 0x1000 * self.wram_bank],
            0xe000..=0xfdff => self.lb(a - 0x2000),
            0xfe00..=0xfe9f => self.lb_odma(a),
            0xfea0..=0xfeff => 0xff,
            0xff00 => self.joypad.lb(a),
            0xff01..=0xff02 => self.serial.lb(a),
            0xff04..=0xff07 => self.timer.lb(a),
            0xff0f => self.intr.borrow().lb(0xff0f),
            0xff10..=0xff3f => self.apu.lb(a),
            0xff40..=0xff45 => self.gpu.lb(a),
            0xff46 => self.dma.o.lb(a),
            0xff47..=0xff4b => self.gpu.lb(a),
            0xff4c..=0xff70 => match self.term {
                Term::DMG => 0xff,
                Term::CGB => match a {
                    0xff4d => 0x7e | ((self.speed == 2) as u8) << 7 | self.speed_switch as u8,
                    0xff4f => self.gpu.lb(a),
                    0xff51..=0xff55 => self.hdma.lb(a),
                    0xff68..=0xff6b => self.gpu.lb(a),
                    0xff70 => self.wram_bank as u8,
                    _ => 0xff,
                },
            },
            0xff80..=0xfffe => self.hram[a as usize - 0xff80],
            0xffff => self.intr.borrow().lb(0xffff),
            _ => 0xff,
        }
    }

    fn sb(&mut self, a: u16, v: u8) {
        match a {
            0x0000..=0x7fff => self.cartridge.sb(a, v),
            0x8000..=0x9fff => self.gpu.sb(a, v),
            0xa000..=0xbfff => self.cartridge.sb(a, v),
            0xc000..=0xcfff => self.wram[a as usize - 0xc000] = v,
            0xd000..=0xdfff => self.wram[a as usize - 0xd000 + 0x1000 * self.wram_bank] = v,
            0xe000..=0xfdff => self.sb(a - 0x2000, v),
            0xfe00..=0xfe9f => {
                // CPU writes to OAM are blocked while OAM DMA is active (bus conflict).
                let cnt = *self.dma.o.cnt.borrow();
                if cnt > 0 && cnt <= 640 {
                    return;
                }
                self.gpu.sb(a, v);
            }
            0xfea0..=0xfeff => {}
            0xff00 => self.joypad.sb(a, v),
            0xff01..=0xff02 => self.serial.sb(a, v),
            0xff04..=0xff07 => self.timer.sb(a, v),
            0xff0f => self.intr.borrow_mut().sb(0xff0f, v),
            0xff10..=0xff3f => {
                self.apu.sdiv_cache = self.glo.borrow().sdiv;
                self.apu.sb(a, v);
            }
            0xff40..=0xff45 => self.gpu.sb(a, v),
            0xff46 => self.dma.o.sb(a, v),
            0xff47..=0xff4b => self.gpu.sb(a, v),
            0xff4c..=0xff70 => match self.term {
                Term::DMG => {}
                Term::CGB => match a {
                    0xff4d => self.speed_switch = v & 0x01 != 0,
                    0xff4f => self.gpu.sb(a, v),
                    0xff51..=0xff55 => self.hdma.sb(a, v),
                    0xff68..=0xff6b => self.gpu.sb(a, v),
                    0xff70 => self.wram_bank = (v as usize & 0x7).max(1),
                    _ => {}
                },
            },
            0xff80..=0xfffe => self.hram[a as usize - 0xff80] = v,
            0xffff => self.intr.borrow_mut().sb(0xffff, v),
            _ => {}
        }
    }
}

impl Mmu {
    pub fn tick(&mut self, cycles: u32) {
        self.advance_clock(cycles);
    }

    fn run_dma(&mut self) -> u32 {
        if !self.hdma.active {
            return 0;
        }
        match self.hdma.mode {
            HdmaMode::Gdma => {
                let len = u32::from(self.hdma.remain) + 1;
                for _ in 0..len {
                    self.run_dma_hrampart();
                }
                self.hdma.active = false;
                len * 8
            }
            HdmaMode::Hdma => {
                if !self.gpu.h_blank {
                    return 0;
                }
                self.run_dma_hrampart();
                if self.hdma.remain == 0x7f {
                    self.hdma.active = false;
                }
                8
            }
        }
    }

    fn run_dma_hrampart(&mut self) {
        let mmu_src = self.hdma.src;
        for i in 0..0x10 {
            let b: u8 = self.lb(mmu_src + i);
            self.gpu.sb(self.hdma.dst + i, b);
        }
        self.hdma.src += 0x10;
        self.hdma.dst += 0x10;
        if self.hdma.remain == 0 {
            self.hdma.remain = 0x7f;
        } else {
            self.hdma.remain -= 1;
        }
    }
}
