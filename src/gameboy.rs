use super::convention::{STEP_CYCLES, STEP_TIME};
use super::cpu::Cpu;
use super::mmu::Mmu;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::thread;
use std::time;

pub struct GameBoy {
    pub mmu: Rc<RefCell<Mmu>>,
    pub cpu: Cpu,
    pub spd: u32,
    c: u32,
    z: time::Instant,
}

impl GameBoy {
    pub fn power_up(path: impl AsRef<Path>) -> Self {
        let mmu = Rc::new(RefCell::new(Mmu::power_up(path)));
        let cpu = Cpu::power_up(mmu.borrow().term, mmu.clone());
        Self { mmu, cpu, spd: 1, c: 0, z: time::Instant::now() }
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
        let cycles = self.cpu.step();
        let cycles = self.mmu.borrow_mut().next(cycles);
        self.c += cycles;
        cycles
    }
}
