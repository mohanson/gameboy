// A memory management unit (MMU), sometimes called paged memory management unit (PMMU), is a computer hardware unit
// having all memory references passed through itself, primarily performing the translation of virtual memory addresses
// to physical addresses.
use super::apu::Apu;
use super::cartridge::Cartridge;
use super::convention::{Memory, Term};
use super::gpu::{Gpu, Hdma, HdmaMode};
use super::interrupt::Interrupt;
use super::joypad::Joypad;
use super::rng;
use super::serial::Serial;
use super::timer::Timer;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

pub struct Mmu {
    pub apu: Apu,
    pub cartridge: Cartridge,
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
    // OAM DMA state
    // oam_dma_reg: last byte written to $FF46 (always returned on reads)
    pub oam_dma_reg: u8,
    // oam_dma_countdown: T-cycles remaining.
    //   0             = no DMA
    //   1..=640       = active phase (OAM bus blocked, returns 0xFF)
    //   641..=652     = 3-M-cycle startup delay (OAM still accessible)
    pub oam_dma_countdown: u32,
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
        let intr = Rc::new(RefCell::new(Interrupt::power_up()));
        let mut r = Self {
            apu: Apu::power_up(48000),
            cartridge: cart,
            gpu: Gpu::power_up(term, intr.clone()),
            hdma: Hdma::power_up(),
            hram: [0x00; 0x7f],
            intr: intr.clone(),
            joypad: Joypad::power_up(intr.clone()),
            serial: Serial::power_up(term),
            term,
            timer: Timer::power_up(term, intr.clone()),
            wram: [0x00; 0x8000],
            wram_bank: 0x01,
            oam_dma_reg: 0xff,
            oam_dma_countdown: 0,
        };
        r.sb(0xff10, 0x80);
        r.sb(0xff11, 0xbf);
        r.sb(0xff12, 0xf3);
        r.sb(0xff13, 0xff);
        r.sb(0xff14, 0xbf);
        r.sb(0xff16, 0x3f);
        r.sb(0xff17, 0x00);
        r.sb(0xff18, 0xff);
        r.sb(0xff19, 0xbf);
        r.sb(0xff1a, 0x7f);
        r.sb(0xff1b, 0xff);
        r.sb(0xff1c, 0x9f);
        r.sb(0xff1d, 0xff);
        r.sb(0xff1e, 0xbf);
        r.sb(0xff20, 0xff);
        r.sb(0xff21, 0x00);
        r.sb(0xff22, 0x00);
        r.sb(0xff23, 0xbf);
        r.sb(0xff24, 0x77);
        r.sb(0xff25, 0xf3);
        r.sb(0xff26, 0xf1);
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
    pub fn next(&mut self, cycles: u32) -> u32 {
        let cycles = cycles + self.run_dma();
        self.advance_oam_dma(cycles);
        self.timer.tick(cycles);
        self.gpu.next(cycles);
        self.apu.next(cycles);
        cycles
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
            0xfe00..=0xfe9f => {
                // During active OAM DMA phase the OAM bus is occupied; CPU sees 0xFF.
                if self.oam_dma_countdown > 0 && self.oam_dma_countdown <= 640 { 0xff } else { self.gpu.lb(a) }
            }
            0xfea0..=0xfeff => 0xff,
            0xff00 => self.joypad.lb(a),
            0xff01..=0xff02 => self.serial.lb(a),
            0xff04..=0xff07 => self.timer.lb(a),
            0xff0f => self.intr.borrow().lb(0xff0f),
            0xff10..=0xff3f => self.apu.lb(a),
            0xff40..=0xff45 => self.gpu.lb(a),
            0xff46 => self.oam_dma_reg,
            0xff47..=0xff4b => self.gpu.lb(a),
            0xff4c..=0xff70 => match self.term {
                Term::DMG => 0xff,
                Term::CGB => match a {
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
            0xfe00..=0xfe9f => self.gpu.sb(a, v),
            0xfea0..=0xfeff => {}
            0xff00 => self.joypad.sb(a, v),
            0xff01..=0xff02 => self.serial.sb(a, v),
            0xff04..=0xff07 => self.timer.sb(a, v),
            0xff0f => self.intr.borrow_mut().sb(0xff0f, v),
            0xff10..=0xff3f => self.apu.sb(a, v),
            0xff40..=0xff45 => self.gpu.sb(a, v),
            0xff46 => self.start_oam_dma(v),
            0xff47..=0xff4b => self.gpu.sb(a, v),
            0xff4c..=0xff70 => match self.term {
                Term::DMG => {}
                Term::CGB => match a {
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
    fn start_oam_dma(&mut self, v: u8) {
        // Writing to $FF46 stores the source page and controls the DMA countdown.
        //
        // Timing (DMG):
        //   Fresh start (no DMA running, countdown == 0):
        //     - 3 M-cycle startup delay (12 T-cycles): OAM bus still accessible
        //     - 160 M-cycle active phase (640 T-cycles): OAM reads return 0xFF
        //     - Total countdown = 652
        //   Restart during startup delay (countdown 641..=652):
        //     - Keep existing countdown unchanged; OAM delay window continues normally.
        //   Restart during active phase (countdown 1..=640):
        //     - Reset countdown to 652 (full delay+active); the new 160-byte transfer runs,
        //       keeping OAM blocked without interruption. The 12T startup delay simply
        //       compensates for mmu.next(cycles) that runs after the write instruction.
        self.oam_dma_reg = v;
        if self.oam_dma_countdown == 0 || self.oam_dma_countdown <= 640 {
            // Fresh start or restart during active phase: full countdown.
            self.oam_dma_countdown = 652;
        }
        // Restart during delay phase (641..=652): do nothing.
    }

    /// Read a byte for the DMA controller itself, bypassing the CPU-visible OAM block.
    fn dma_lb(&self, a: u16) -> u8 {
        match a {
            0x0000..=0x7fff => self.cartridge.lb(a),
            0x8000..=0x9fff => self.gpu.lb(a),
            0xa000..=0xbfff => self.cartridge.lb(a),
            0xc000..=0xcfff => self.wram[a as usize - 0xc000],
            0xd000..=0xdfff => self.wram[a as usize - 0xd000 + 0x1000 * self.wram_bank],
            // Echo RAM: on DMG the DMA controller extends echo mapping through $FFFF,
            // so $E000-$FFFF maps back to $C000-$DFFF (via -$2000).
            0xe000..=0xffff => self.dma_lb(a - 0x2000),
        }
    }

    fn advance_oam_dma(&mut self, cycles: u32) {
        if self.oam_dma_countdown == 0 {
            return;
        }
        let prev = self.oam_dma_countdown;
        self.oam_dma_countdown = self.oam_dma_countdown.saturating_sub(cycles);
        let new = self.oam_dma_countdown;

        // Active phase spans countdown values 640 down to 1.
        // Byte i is transferred during the M-cycle when countdown is in (640-4i, 640-4(i-1)].
        // i.e. byte 0 when countdown goes 640->636, byte 1 when 636->632, ...
        // We copy all bytes whose M-cycle window falls within the range consumed this call.
        let active_top: u32 = 640;
        // Clamp to active phase
        let start = prev.min(active_top); // highest countdown in active phase before this call
        let end = new; // new countdown after this call
        if start > end {
            // Bytes to copy: index i where active_top - 4*(i+1) < start and active_top - 4*i >= end
            // i.e. 4*i < active_top - end  and  4*(i+1) > active_top - start
            // => i < (active_top - end) / 4  and  i >= (active_top - start) / 4
            let i_start = (active_top.saturating_sub(start) + 3) / 4; // round up
            let i_end = (active_top.saturating_sub(end) + 3) / 4; // exclusive upper
            let src_page = (self.oam_dma_reg as u16) << 8;
            for i in i_start..i_end.min(160) {
                let b = self.dma_lb(src_page | i as u16);
                self.gpu.oam[i as usize] = b;
            }
        }
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
