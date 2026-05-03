use super::cpu::Rtc;
use super::mmu::Mmu;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

pub struct GameBoy {
    pub mmu: Rc<RefCell<Mmu>>,
    pub cpu: Rtc,
}

impl GameBoy {
    pub fn power_up(path: impl AsRef<Path>) -> Self {
        let mmu = Rc::new(RefCell::new(Mmu::power_up(path)));
        let cpu = Rtc::power_up(mmu.borrow().term, mmu.clone());
        Self { mmu, cpu }
    }

    pub fn step(&mut self) -> u32 {
        let cycles = self.cpu.step();
        let cycles = self.mmu.borrow_mut().next(cycles);
        cycles
    }
}
