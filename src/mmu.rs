// A memory management unit (MMU), sometimes called paged memory management unit (PMMU), is a computer hardware unit
// having all memory references passed through itself, primarily performing the translation of virtual memory addresses
// to physical addresses.
use crate::apu::Apu;
use crate::cartridge::Cartridge;
use crate::convention::{Global, Memory, Term, Ticker};
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
    pub gpu: Gpu,
    pub hdma: Hdma,
    pub hram: [u8; 0x7f],
    pub intr: Interrupt,
    pub joypad: Joypad,
    pub oam_dma_reg: u8,
    pub oam_dma_cnt: u32,
    pub oam_dma_pending: bool,
    pub rom: Cartridge,
    pub serial: Serial,
    pub speed: u8,
    pub speed_switch: bool,
    pub timer: Timer,
    pub wram: [u8; 0x8000],
    pub wram_bank: usize,
}

impl Mmu {
    pub fn power_up(glo: Rc<RefCell<Global>>, rom: Cartridge) -> Self {
        let mut r = Self {
            glo: glo.clone(),
            apu: Apu::power_up(glo.clone(), 48000),
            gpu: Gpu::power_up(glo.clone()),
            hdma: Hdma::power_up(),
            hram: [0x00; 0x7f],
            intr: Interrupt::power_up(glo.clone()),
            joypad: Joypad::power_up(glo.clone()),
            oam_dma_reg: 0xff,
            oam_dma_cnt: 0,
            oam_dma_pending: false,
            rom,
            serial: Serial::power_up(glo.clone()),
            speed: 1,
            speed_switch: false,
            timer: Timer::power_up(glo.clone()),
            wram: [0x00; 0x8000],
            wram_bank: 0x01,
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

    pub fn notify_spd(&mut self) -> bool {
        self.speed = 3 - self.speed;
        self.speed_switch = false;
        true
    }

    /// Called once per CPU instruction after all per-M-cycle ticks have already advanced
    /// timer/GPU/APU. Runs HDMA and resets the h_blank edge signal.
    pub fn next(&mut self) -> u32 {
        let hdma_cycles = self.run_dma();
        self.gpu.h_blank = false;
        if hdma_cycles > 0 {
            self.timer.tick(hdma_cycles as u16);
            let video_cycles = self.video_cycles(hdma_cycles);
            self.gpu.next(video_cycles);
            self.apu.next(video_cycles);
            self.gpu.h_blank = false;
        }
        hdma_cycles
    }

    fn advance_clock(&mut self, cycles: u32) {
        self.timer.tick(cycles as u16);
        self.serial.tick(cycles as u16);
        let video_cycles = self.video_cycles(cycles);
        self.gpu.next(video_cycles);
        self.apu.next(video_cycles);
        // OAM DMA: advance the countdown and copy any bytes that fall inside this M-cycle.
        let old_cnt = self.oam_dma_cnt;
        if self.oam_dma_pending {
            self.oam_dma_pending = false;
            if self.oam_dma_cnt <= 639 {
                self.oam_dma_cnt = 648; // 2 M-cycle startup delay (fresh start or restart mid-copy)
            }
        }
        if self.oam_dma_cnt != 0 {
            self.oam_dma_cnt = self.oam_dma_cnt.saturating_sub(cycles as u32);
        }
        if old_cnt != 0 && old_cnt.min(640) > self.oam_dma_cnt {
            let first = (640u32.saturating_sub(old_cnt.min(640)) + 3) / 4;
            let last = (640u32.saturating_sub(self.oam_dma_cnt) + 3) / 4;
            let src_page = (self.oam_dma_reg as u16) << 8;
            let wram_bank = self.wram_bank;
            let gpu = &mut self.gpu;
            let rom = &self.rom;
            let wram = &self.wram;
            for i in first..last.min(160) {
                let src = src_page | i as u16;
                let src = if src <= 0xdfff { src } else { src - 0x2000 };
                let b = match src {
                    0x0000..=0x7fff | 0xa000..=0xbfff => rom.lb(src),
                    0x8000..=0x9fff => gpu.lb(src),
                    0xc000..=0xcfff => wram[src as usize - 0xc000],
                    0xd000..=0xdfff => wram[src as usize - 0xd000 + 0x1000 * wram_bank],
                    _ => 0xff,
                };
                gpu.sb(0xfe00 + i as u16, b);
            }
        }
    }

    fn oam_dma_is_active(&self) -> bool {
        self.oam_dma_cnt > 0 && self.oam_dma_cnt <= 640
    }

    pub fn lb_odma(&self, a: u16) -> u8 {
        match a {
            0xfe00..=0xfe9f => {
                if self.oam_dma_is_active() {
                    return 0xff;
                }
                self.gpu.lb(a)
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
            0xff04..=0xff07 => self.timer.lb(a),
            0xff0f => self.intr.lb(0xff0f),
            0xff10..=0xff3f => self.apu.lb(a),
            0xff40..=0xff45 => self.gpu.lb(a),
            0xff46 => self.oam_dma_reg,
            0xff47..=0xff4b => self.gpu.lb(a),
            0xff4c..=0xff70 => match self.glo.borrow().term {
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
                if self.oam_dma_is_active() {
                    return;
                }
                self.gpu.sb(a, v);
            }
            0xfea0..=0xfeff => {}
            0xff00 => self.joypad.sb(a, v),
            0xff01..=0xff02 => self.serial.sb(a, v),
            0xff04..=0xff07 => self.timer.sb(a, v),
            0xff0f => self.intr.sb(0xff0f, v),
            0xff10..=0xff3f => {
                self.apu.sdiv_cache = self.glo.borrow().sdiv;
                self.apu.sb(a, v);
            }
            0xff40..=0xff45 => self.gpu.sb(a, v),
            0xff46 => {
                self.oam_dma_reg = v;
                self.oam_dma_pending = true;
            }
            0xff47..=0xff4b => self.gpu.sb(a, v),
            0xff4c..=0xff70 => match self.glo.borrow().term {
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
            0xffff => self.intr.sb(0xffff, v),
            _ => {}
        }
    }
}

impl Mmu {
    pub fn tick(&mut self, cycles: u32) {
        self.advance_clock(cycles);
        self.next();
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
