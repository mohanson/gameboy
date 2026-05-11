use crate::cartridge::Cartridge;
use crate::convention::{Global, Memory, STEP_CYCLES, STEP_TIME, Term};
use crate::cpu::Cpu;
use crate::mmu::Mmu;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::thread;
use std::time;

pub struct GameBoy {
    pub cpu: Cpu,
    pub glo: Rc<RefCell<Global>>,
    pub mmu: Rc<RefCell<Mmu>>,
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
        Self { cpu, glo, mmu, spd: 1, c: 0, z: time::Instant::now() }
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
        let origin = self.glo.borrow().sdiw;
        self.cpu.step();
        let cycles = self.glo.borrow().sdiw.wrapping_sub(origin);
        self.c += cycles as u32;
        cycles as u32
    }
}
