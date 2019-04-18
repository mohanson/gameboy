use super::convention::Term;
use super::cpu::Cpu;
use super::joypad::JoypadKey;
use super::memory::Memory;
use super::mmunit::MemoryManagementUnit;
use super::sound::{AudioPlayer, Sound};
use std::path::Path;

pub struct MotherBoard {
    pub mmu: MemoryManagementUnit,
    pub cpu: Cpu,
}

impl MotherBoard {
    pub fn power_up(term: Term, path: impl AsRef<Path>) -> Self {
        Self {
            mmu: MemoryManagementUnit::power_up(term, path),
            cpu: Cpu::power_up(term),
        }
    }

    pub fn do_cycle(&mut self) -> u32 {
        if self.mmu.get(self.cpu.reg.pc) == 0x10 {
            self.mmu.switch_speed();
        }
        let cycles = self.cpu.next(&mut self.mmu) * 4;
        self.mmu.do_cycle(cycles as u32);
        cycles as u32
    }

    pub fn check_and_reset_gpu_updated(&mut self) -> bool {
        let result = self.mmu.gpu.updated;
        self.mmu.gpu.updated = false;
        result
    }

    pub fn get_gpu_data(&self) -> Vec<u8> {
        let mut d = vec![];
        for l in self.mmu.gpu.data.iter() {
            for w in l.iter() {
                d.extend(w);
            }
        }
        d
    }

    pub fn enable_audio(&mut self, player: Box<AudioPlayer>) {
        self.mmu.sound = Some(Sound::new(player));
    }

    pub fn sync_audio(&mut self) {
        if let Some(ref mut sound) = self.mmu.sound {
            sound.sync();
        }
    }

    pub fn keyup(&mut self, key: JoypadKey) {
        self.mmu.joypad.keyup(key);
    }

    pub fn keydown(&mut self, key: JoypadKey) {
        self.mmu.joypad.keydown(key);
    }

    pub fn romname(&self) -> String {
        self.mmu.cartridge.rom_name()
    }
}
