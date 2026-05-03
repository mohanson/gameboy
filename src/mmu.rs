// A memory management unit (MMU), sometimes called paged memory management unit (PMMU), is a computer hardware unit
// having all memory references passed through itself, primarily performing the translation of virtual memory addresses
// to physical addresses.
use super::apu::Apu;
use super::cartridge::Cartridge;
use super::convention::{Memory, Term};
use super::gpu::{Gpu, Hdma, HdmaMode};
use super::interrupt::Interrupt;
use super::joypad::Joypad;
use super::serial::Serial;
use super::timer::Timer;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

pub struct Mmu {
    pub cartridge: Cartridge,
    pub apu: Apu,
    pub gpu: Gpu,
    pub joypad: Joypad,
    pub serial: Serial,
    pub term: Term,
    pub timer: Timer,
    intf: Rc<RefCell<Interrupt>>,
    hdma: Hdma,
    hram: [u8; 0x7f],
    wram: [u8; 0x8000],
    wram_bank: usize,
}

impl Mmu {
    pub fn power_up(path: impl AsRef<Path>) -> Self {
        let cart = Cartridge::power_up(path);
        let term = match cart.lb(0x0143) & 0x80 {
            0x00 => Term::DMG,
            0x80 => Term::CGB,
            _ => unreachable!(),
        };
        rog::debugln!("GameBoy term is {}", term);
        let intf = Rc::new(RefCell::new(Interrupt::power_up()));
        let mut r = Self {
            cartridge: cart,
            apu: Apu::power_up(48000),
            gpu: Gpu::power_up(term, intf.clone()),
            joypad: Joypad::power_up(intf.clone()),
            serial: Serial::power_up(term),
            term,
            timer: Timer::power_up(term, intf.clone()),
            intf: intf.clone(),
            hdma: Hdma::power_up(),
            hram: [0x00; 0x7f],
            wram: [0x00; 0x8000],
            wram_bank: 0x01,
        };
        r.sb(0xff10, 0x80);
        r.sb(0xff11, 0xbf);
        r.sb(0xff12, 0xf3);
        r.sb(0xff14, 0xbf);
        r.sb(0xff16, 0x3f);
        r.sb(0xff16, 0x3f);
        r.sb(0xff17, 0x00);
        r.sb(0xff19, 0xbf);
        r.sb(0xff1a, 0x7f);
        r.sb(0xff1b, 0xff);
        r.sb(0xff1c, 0x9f);
        r.sb(0xff1e, 0xff);
        r.sb(0xff20, 0xff);
        r.sb(0xff21, 0x00);
        r.sb(0xff22, 0x00);
        r.sb(0xff23, 0xbf);
        r.sb(0xff24, 0x77);
        r.sb(0xff25, 0xf3);
        r.sb(0xff26, 0xf1);
        r.sb(0xff40, 0x91);
        r.sb(0xff42, 0x00);
        r.sb(0xff43, 0x00);
        r.sb(0xff45, 0x00);
        r.sb(0xff47, 0xfc);
        r.sb(0xff48, 0xff);
        r.sb(0xff49, 0xff);
        r.sb(0xff4a, 0x00);
        r.sb(0xff4b, 0x00);
        r
    }
}

impl Mmu {
    pub fn next(&mut self, cycles: u32) -> u32 {
        let vram_cycles = self.run_dma();
        let gpu_cycles = cycles + vram_cycles;
        self.timer.tick(gpu_cycles);
        self.gpu.next(gpu_cycles);
        self.apu.next(gpu_cycles);
        gpu_cycles
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

impl Memory for Mmu {
    fn lb(&self, a: u16) -> u8 {
        match a {
            0x0000..=0x7fff => self.cartridge.lb(a),
            0x8000..=0x9fff => self.gpu.lb(a),
            0xa000..=0xbfff => self.cartridge.lb(a),
            0xc000..=0xcfff => self.wram[a as usize - 0xc000],
            0xd000..=0xdfff => self.wram[a as usize - 0xd000 + 0x1000 * self.wram_bank],
            0xe000..=0xefff => self.wram[a as usize - 0xe000],
            0xf000..=0xfdff => self.wram[a as usize - 0xf000 + 0x1000 * self.wram_bank],
            0xfe00..=0xfe9f => self.gpu.lb(a),
            0xfea0..=0xfeff => 0x00,
            0xff00 => self.joypad.lb(a),
            0xff01..=0xff02 => self.serial.lb(a),
            0xff04..=0xff07 => self.timer.lb(a),
            0xff0f => self.intf.borrow().lb(0xff0f),
            0xff10..=0xff3f => self.apu.lb(a),
            0xff4c..=0xff7f if self.term == Term::DMG => 0xff,
            0xff40..=0xff45 | 0xff47..=0xff4b | 0xff4f => self.gpu.lb(a),
            0xff51..=0xff55 => self.hdma.lb(a),
            0xff68..=0xff6b => self.gpu.lb(a),
            0xff70 => self.wram_bank as u8,
            0xff80..=0xfffe => self.hram[a as usize - 0xff80],
            0xffff => self.intf.borrow().lb(0xffff),
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
            0xe000..=0xefff => self.wram[a as usize - 0xe000] = v,
            0xf000..=0xfdff => self.wram[a as usize - 0xf000 + 0x1000 * self.wram_bank] = v,
            0xfe00..=0xfe9f => self.gpu.sb(a, v),
            0xfea0..=0xfeff => {}
            0xff00 => self.joypad.sb(a, v),
            0xff01..=0xff02 => self.serial.sb(a, v),
            0xff04..=0xff07 => self.timer.sb(a, v),
            0xff10..=0xff3f => self.apu.sb(a, v),
            0xff46 => {
                // Writing to this register launches a DMA transfer from ROM or RAM to OAM memory (sprite attribute
                // table).
                // See: http://gbdev.gg8.se/wiki/articles/Video_Display#FF46_-_DMA_-_DMA_Transfer_and_Start_Address_.28R.2FW.29
                assert!(v <= 0xf1);
                let base = u16::from(v) << 8;
                for i in 0..0xa0 {
                    let b = self.lb(base + i);
                    self.sb(0xfe00 + i, b);
                }
            }
            0xff4c..=0xff7f if self.term == Term::DMG => {}
            0xff40..=0xff45 | 0xff47..=0xff4b | 0xff4f => self.gpu.sb(a, v),
            0xff51..=0xff55 => self.hdma.sb(a, v),
            0xff68..=0xff6b => self.gpu.sb(a, v),
            0xff0f => self.intf.borrow_mut().sb(0xff0f, v),
            0xff70 => {
                self.wram_bank = match v & 0x7 {
                    0 => 1,
                    n => n as usize,
                };
            }
            0xff80..=0xfffe => self.hram[a as usize - 0xff80] = v,
            0xffff => self.intf.borrow_mut().sb(0xffff, v),
            _ => {}
        }
    }
}
