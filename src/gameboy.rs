use crate::cartridge::Cartridge;
use crate::convention::{Global, Memory, STEP_CYCLES, STEP_TIME, Term};
use crate::cpu::Cpu;
use crate::dma::Dma;
use crate::mmu::Mmu;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::thread;
use std::time;

pub struct GameBoy {
    pub mmu: Rc<RefCell<Mmu>>,
    pub cpu: Cpu,
    pub dma: Dma,
    pub glo: Rc<RefCell<Global>>,
    pub spd: u32,
    c: u32,
    z: time::Instant,
}

impl GameBoy {
    pub fn power_up(path: impl AsRef<Path>) -> Self {
        let rom = Cartridge::power_up(path);
        let glo = Global::power_up().share();
        glo.borrow_mut().term = match rom.lb(0x0143) & 0xC0 {
            0x00 => Term::DMG,
            0x80 => Term::DMG,
            0xC0 => Term::CGB,
            _ => unreachable!(),
        };
        rog::debugln!("GameBoy term is {}", glo.borrow().term);
        let mmu = Rc::new(RefCell::new(Mmu::power_up(glo.clone(), rom)));
        let cpu = Cpu::power_up(glo.clone(), mmu.clone());
        let mut dma = Dma::power_up(mmu.clone());
        dma.o.cnt = mmu.borrow().dma.o.cnt.clone();
        dma.o.reg = mmu.borrow().dma.o.reg.clone();
        dma.o.sig = mmu.borrow().dma.o.sig.clone();
        Self { mmu, cpu, dma, glo, spd: 1, c: 0, z: time::Instant::now() }
    }

    pub fn step(&mut self) -> u32 {
        if self.c > STEP_CYCLES {
            self.c -= STEP_CYCLES;
            let now = time::Instant::now();
            let d = now.duration_since(self.z);
            let s = u64::from((STEP_TIME / self.spd).saturating_sub(d.as_millis() as u32));
            thread::sleep(time::Duration::from_millis(s));
            self.z = self.z.checked_add(time::Duration::from_millis(u64::from(STEP_TIME / self.spd))).unwrap();
            // If now is after the just updated target frame time, reset to avoid drift.
            if now.checked_duration_since(self.z).is_some() {
                self.z = now;
            }
        }
        let old_cnt = *self.dma.o.cnt.borrow();
        let cycles = self.cpu.step();
        let new_cnt = *self.dma.o.cnt.borrow();
        self.dma.o.do_copies(old_cnt, new_cnt);
        let hdma_cycles = self.mmu.borrow_mut().next();
        self.c += cycles + hdma_cycles;
        cycles + hdma_cycles
    }
}
