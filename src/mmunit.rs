// A memory management unit (MMU), sometimes called paged memory management unit (PMMU), is a computer hardware unit
// having all memory references passed through itself, primarily performing the translation of virtual memory addresses
// to physical addresses.
use super::apu::Apu;
use super::cartridge::{self, Cartridge};
use super::convention::Term;
use super::gpu::{Gpu, Hdma, HdmaMode};
use super::intf::Intf;
use super::joypad::Joypad;
use super::memory::Memory;
use super::serial::Serial;
use super::timer::Timer;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Speed {
    Normal = 0x01,
    Double = 0x02,
}

pub struct Mmunit {
    pub cartridge: Box<dyn Cartridge>,
    pub apu: Apu,
    pub gpu: Gpu,
    pub joypad: Joypad,
    pub serial: Serial,
    pub shift: bool,
    pub speed: Speed,
    pub term: Term,
    pub timer: Timer,
    inte: u8,
    intf: Rc<RefCell<Intf>>,
    hdma: Hdma,
    hram: [u8; 0x7f],
    wram: [u8; 0x8000],
    wram_bank: usize,
}

impl Mmunit {
    pub fn power_up(path: impl AsRef<Path>) -> Self {
        let cart = cartridge::power_up(path);
        let term = match cart.get(0x0143) & 0x80 {
            0x80 => Term::GBC,
            _ => Term::GB,
        };
        let intf = Rc::new(RefCell::new(Intf::power_up()));
        let mut r = Self {
            cartridge: cart,
            apu: Apu::power_up(48000),
            gpu: Gpu::power_up(term, intf.clone()),
            joypad: Joypad::power_up(intf.clone()),
            serial: Serial::power_up(intf.clone()),
            shift: false,
            speed: Speed::Normal,
            term,
            timer: Timer::power_up(intf.clone()),
            inte: 0x00,
            intf: intf.clone(),
            hdma: Hdma::power_up(),
            hram: [0x00; 0x7f],
            wram: [0x00; 0x8000],
            wram_bank: 0x01,
        };
        r.set(0xff05, 0x00);
        r.set(0xff06, 0x00);
        r.set(0xff07, 0x00);
        r.set(0xff10, 0x80);
        r.set(0xff11, 0xbf);
        r.set(0xff12, 0xf3);
        r.set(0xff14, 0xbf);
        r.set(0xff16, 0x3f);
        r.set(0xff16, 0x3f);
        r.set(0xff17, 0x00);
        r.set(0xff19, 0xbf);
        r.set(0xff1a, 0x7f);
        r.set(0xff1b, 0xff);
        r.set(0xff1c, 0x9f);
        r.set(0xff1e, 0xff);
        r.set(0xff20, 0xff);
        r.set(0xff21, 0x00);
        r.set(0xff22, 0x00);
        r.set(0xff23, 0xbf);
        r.set(0xff24, 0x77);
        r.set(0xff25, 0xf3);
        r.set(0xff26, 0xf1);
        r.set(0xff40, 0x91);
        r.set(0xff42, 0x00);
        r.set(0xff43, 0x00);
        r.set(0xff45, 0x00);
        r.set(0xff47, 0xfc);
        r.set(0xff48, 0xff);
        r.set(0xff49, 0xff);
        r.set(0xff4a, 0x00);
        r.set(0xff4b, 0x00);
        r
    }
}

impl Mmunit {
    pub fn next(&mut self, cycles: u32) -> u32 {
        let cpu_divider = self.speed as u32;
        let vram_cycles = self.run_dma();
        let gpu_cycles = cycles / cpu_divider + vram_cycles;
        let cpu_cycles = cycles + vram_cycles * cpu_divider;
        self.timer.next(cpu_cycles);
        self.gpu.next(gpu_cycles);
        self.apu.next(gpu_cycles);
        gpu_cycles
    }

    pub fn switch_speed(&mut self) {
        if self.shift {
            if self.speed == Speed::Double {
                self.speed = Speed::Normal;
            } else {
                self.speed = Speed::Double;
            }
        }
        self.shift = false;
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
            let b: u8 = self.get(mmu_src + i);
            self.gpu.set(self.hdma.dst + i, b);
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

impl Memory for Mmunit {
    fn get(&self, a: u16) -> u8 {
        match a {
            0x0000..=0x7fff => self.cartridge.get(a),
            0x8000..=0x9fff => self.gpu.get(a),
            0xa000..=0xbfff => self.cartridge.get(a),
            0xc000..=0xcfff => self.wram[a as usize - 0xc000],
            0xd000..=0xdfff => self.wram[a as usize - 0xd000 + 0x1000 * self.wram_bank],
            0xe000..=0xefff => self.wram[a as usize - 0xe000],
            0xf000..=0xfdff => self.wram[a as usize - 0xf000 + 0x1000 * self.wram_bank],
            0xfe00..=0xfe9f => self.gpu.get(a),
            0xfea0..=0xfeff => 0x00,
            0xff00 => self.joypad.get(a),
            0xff01..=0xff02 => self.serial.get(a),
            0xff04..=0xff07 => self.timer.get(a),
            0xff0f => self.intf.borrow().data,
            0xff10..=0xff3f => self.apu.get(a),
            0xff4d => {
                let a = if self.speed == Speed::Double { 0x80 } else { 0x00 };
                let b = if self.shift { 0x01 } else { 0x00 };
                a | b
            }
            0xff40..=0xff45 | 0xff47..=0xff4b | 0xff4f => self.gpu.get(a),
            0xff51..=0xff55 => self.hdma.get(a),
            0xff68..=0xff6b => self.gpu.get(a),
            0xff70 => self.wram_bank as u8,
            0xff80..=0xfffe => self.hram[a as usize - 0xff80],
            0xffff => self.inte,
            _ => 0x00,
        }
    }

    fn set(&mut self, a: u16, v: u8) {
        match a {
            0x0000..=0x7fff => self.cartridge.set(a, v),
            0x8000..=0x9fff => self.gpu.set(a, v),
            0xa000..=0xbfff => self.cartridge.set(a, v),
            0xc000..=0xcfff => self.wram[a as usize - 0xc000] = v,
            0xd000..=0xdfff => self.wram[a as usize - 0xd000 + 0x1000 * self.wram_bank] = v,
            0xe000..=0xefff => self.wram[a as usize - 0xe000] = v,
            0xf000..=0xfdff => self.wram[a as usize - 0xf000 + 0x1000 * self.wram_bank] = v,
            0xfe00..=0xfe9f => self.gpu.set(a, v),
            0xfea0..=0xfeff => {}
            0xff00 => self.joypad.set(a, v),
            0xff01..=0xff02 => self.serial.set(a, v),
            0xff04..=0xff07 => self.timer.set(a, v),
            0xff10..=0xff3f => self.apu.set(a, v),
            0xff46 => {
                // Writing to this register launches a DMA transfer from ROM or RAM to OAM memory (sprite attribute
                // table).
                // See: http://gbdev.gg8.se/wiki/articles/Video_Display#FF46_-_DMA_-_DMA_Transfer_and_Start_Address_.28R.2FW.29
                assert!(v <= 0xf1);
                let base = u16::from(v) << 8;
                for i in 0..0xa0 {
                    let b = self.get(base + i);
                    self.set(0xfe00 + i, b);
                }
            }
            0xff4d => self.shift = (v & 0x01) == 0x01,
            0xff40..=0xff45 | 0xff47..=0xff4b | 0xff4f => self.gpu.set(a, v),
            0xff51..=0xff55 => self.hdma.set(a, v),
            0xff68..=0xff6b => self.gpu.set(a, v),
            0xff0f => self.intf.borrow_mut().data = v,
            0xff70 => {
                self.wram_bank = match v & 0x7 {
                    0 => 1,
                    n => n as usize,
                };
            }
            0xff80..=0xfffe => self.hram[a as usize - 0xff80] = v,
            0xffff => self.inte = v,
            _ => {}
        }
    }
}
