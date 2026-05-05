use super::convention::Signal;
use crate::convention::Memory;
use std::cell::RefCell;
use std::rc::Rc;

pub struct OamDma {
    pub mem: Rc<RefCell<dyn Memory>>,
    /// Last value written to $FF46.
    pub reg: u8,
    /// Remaining T-cycles (0 = idle).
    pub countdown: u32,
    /// One-shot signal: fires when MMU receives a write to $FF46.
    pub trigger: Signal,
    /// The source-page byte carried with the trigger.
    pub trigger_val: Rc<RefCell<u8>>,
    /// Shared with MMU: true while the active DMA phase occupies the OAM bus.
    pub oam_blocked: Rc<RefCell<bool>>,
}

impl OamDma {
    pub fn power_up(mem: Rc<RefCell<dyn Memory>>) -> Self {
        Self {
            mem,
            reg: 0xff,
            countdown: 0,
            trigger: Signal::power_up(),
            trigger_val: Rc::new(RefCell::new(0xff)),
            oam_blocked: Rc::new(RefCell::new(false)),
        }
    }

    pub fn dma_lb(&self, a: u16) -> u8 {
        match a {
            0x0000..=0x7fff => self.mem.borrow().lb(a),
            0x8000..=0x9fff => self.mem.borrow().lb(a),
            0xa000..=0xbfff => self.mem.borrow().lb(a),
            0xc000..=0xcfff => self.mem.borrow().lb(a),
            0xd000..=0xdfff => self.mem.borrow().lb(a),
            // Echo RAM: on DMG the DMA controller extends echo mapping through $FFFF,
            // so $E000-$FFFF maps back to $C000-$DFFF (via -$2000).
            0xe000..=0xffff => self.dma_lb(a - 0x2000),
        }
    }

    /// Returns cloned handles for `Mmu` to hold.
    /// Pass the result to `Mmu::power_up` during construction.
    pub fn handles(&self) -> (Signal, Rc<RefCell<u8>>, Rc<RefCell<bool>>) {
        (self.trigger.clone(), self.trigger_val.clone(), self.oam_blocked.clone())
    }

    fn start(&mut self, v: u8) {
        self.reg = v;
        if self.countdown == 0 || self.countdown <= 640 {
            // Fresh start or restart during active phase: full countdown.
            self.countdown = 652;
        }
        // Restart during delay phase (641..=652): countdown unchanged.
    }

    /// Advance the DMA by `cycles` T-cycles, reading source bytes from `mmu`
    /// and writing them directly into OAM.
    ///
    /// Call once per step, after `mmu.next()`.
    pub fn advance(&mut self, cycles: u32) {
        // Consume any $FF46 write that occurred during the last CPU step.
        if self.trigger.get() {
            let v = *self.trigger_val.borrow();
            self.start(v);
        }

        if self.countdown == 0 {
            *self.oam_blocked.borrow_mut() = false;
            return;
        }

        let prev = self.countdown;
        self.countdown = self.countdown.saturating_sub(cycles);
        let new = self.countdown;

        // Reflect new blocked state to MMU.
        *self.oam_blocked.borrow_mut() = new > 0 && new <= 640;

        // Active phase: countdown runs from 640 down to 1.
        // Byte i is transferred during the M-cycle that lowers the countdown
        // through the window (640 - 4*(i+1), 640 - 4*i].
        let active_top: u32 = 640;
        let start = prev.min(active_top);
        let end = new;
        if start > end {
            let i_start = (active_top.saturating_sub(start) + 3) / 4;
            let i_end = (active_top.saturating_sub(end) + 3) / 4;
            let src_page = (self.reg as u16) << 8;
            for i in i_start..i_end.min(160) {
                let b = self.dma_lb(src_page | i as u16);
                self.mem.borrow_mut().sb(0xfe00 + i as u16, b);
            }
        }
    }
}
