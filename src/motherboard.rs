use super::cpu::Cpu;
use super::memory::Memory;
use super::mmunit::MemoryManagementUnit;
use super::sound::{AudioPlayer, Sound};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

pub struct MotherBoard {
    pub mmu: Rc<RefCell<MemoryManagementUnit>>,
    pub cpu: Cpu,
}

impl MotherBoard {
    pub fn power_up(path: impl AsRef<Path>) -> Self {
        let mmu = Rc::new(RefCell::new(MemoryManagementUnit::power_up(path)));
        let cpu = Cpu::power_up(mmu.borrow().term, mmu.clone());
        Self { mmu, cpu }
    }

    pub fn next(&mut self) -> u32 {
        if self.mmu.borrow().get(self.cpu.reg.pc) == 0x10 {
            self.mmu.borrow_mut().switch_speed();
        }
        let cycles = self.cpu.step();
        self.mmu.borrow_mut().next(cycles);
        cycles
    }

    pub fn check_and_reset_gpu_updated(&mut self) -> bool {
        let result = self.mmu.borrow().gpu.updated;
        self.mmu.borrow_mut().gpu.updated = false;
        result
    }

    pub fn enable_audio(&mut self, player: Box<AudioPlayer>) {
        self.mmu.borrow_mut().sound = Some(Sound::new(player));
    }

    pub fn sync_audio(&mut self) {
        if let Some(ref mut sound) = self.mmu.borrow_mut().sound {
            sound.sync();
        }
    }
}
