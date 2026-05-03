use super::cpu::Rtc;
use super::mmu::Mmu;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

pub struct MotherBoard {
    pub mmu: Rc<RefCell<Mmu>>,
    pub cpu: Rtc,
}

impl MotherBoard {
    pub fn power_up(path: impl AsRef<Path>) -> Self {
        let mmu = Rc::new(RefCell::new(Mmu::power_up(path)));
        let cpu = Rtc::power_up(mmu.borrow().term, mmu.clone());
        Self { mmu, cpu }
    }

    pub fn next(&mut self) -> u32 {
        let cycles = self.cpu.step();
        self.mmu.borrow_mut().next(cycles);
        cycles
    }

    pub fn check_and_reset_gpu_updated(&mut self) -> bool {
        let result = self.mmu.borrow().gpu.v_blank;
        self.mmu.borrow_mut().gpu.v_blank = false;
        result
    }
}
