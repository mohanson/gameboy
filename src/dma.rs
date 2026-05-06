use crate::convention::Memory;
use std::cell::RefCell;
use std::rc::Rc;

pub struct Dma {
    pub o: O,
}

impl Dma {
    pub fn power_up(mem: Rc<RefCell<dyn Memory>>) -> Self {
        Self { o: O::power_up(mem) }
    }
}

pub struct O {
    pub cnt: Rc<RefCell<u32>>,
    pub mem: Rc<RefCell<dyn Memory>>,
    pub reg: Rc<RefCell<u8>>,
    pub sig: Rc<RefCell<u8>>,
}

impl Memory for O {
    fn lb(&self, a: u16) -> u8 {
        assert_eq!(a, 0xff46);
        self.reg.borrow().clone()
    }

    fn sb(&mut self, a: u16, v: u8) {
        assert_eq!(a, 0xff46);
        self.reg.replace(v);
        self.sig.replace(0x01);
    }
}

impl O {
    pub fn power_up(mem: Rc<RefCell<dyn Memory>>) -> Self {
        Self {
            cnt: Rc::new(RefCell::new(0x00)),
            mem,
            reg: Rc::new(RefCell::new(0xff)),
            sig: Rc::new(RefCell::new(0x00)),
        }
    }

    pub fn tick(&mut self, cycles: u32) {
        let old = self.cnt.borrow().clone();
        self.advance_counter(cycles);
        let new = self.cnt.borrow().clone();
        self.do_copies(old, new);
    }
    /// Advance only the DMA counter (no byte copies). Called per M-cycle from mmu::advance_clock
    /// so that lb_odma sees cycle-accurate DMA active/inactive state within an instruction.
    pub fn advance_counter(&mut self, cycles: u32) {
        if *self.sig.borrow() != 0x00 {
            self.sig.replace(0x00);
            let cnt = *self.cnt.borrow();
            if cnt == 0 {
                // Fresh DMA start: 1 M-cycle startup delay before OAM becomes inaccessible.
                // cnt=648 → after this M-cycle's 4T: cnt=644 (> 640, not active yet).
                // One further M-cycle later cnt=640, DMA becomes active.
                self.cnt.replace(648);
            } else if cnt < 640 {
                // Restart while DMA is actively blocking OAM (cnt in 1..639).
                // Reload full 640T period; same startup model as fresh (cnt=648 → 644 after tick).
                self.cnt.replace(648);
            }
            // cnt == 640: first active cycle; a write here does not reset the counter.
            // cnt > 640: startup phase from a prior write; ignore the new write.
        }
        if *self.cnt.borrow() == 0 {
            return;
        }
        let old = self.cnt.borrow().clone();
        let new = old.saturating_sub(cycles);
        self.cnt.replace(new);
    }

    /// Perform OAM byte copies for the counter range [old_cnt, new_cnt].
    /// Called once per instruction from gameboy::step after cpu.step() completes.
    pub fn do_copies(&mut self, old_cnt: u32, new_cnt: u32) {
        if old_cnt == 0 {
            return;
        }
        let ori = old_cnt.min(640);
        let end = new_cnt;
        if ori <= end {
            return;
        }
        let src = (640u32.saturating_sub(ori) + 3) / 4;
        let dst = (640u32.saturating_sub(end) + 3) / 4;
        let src_page = (*self.reg.borrow() as u16) << 8;
        for i in src..dst.min(160) {
            let a = src_page | i as u16;
            let b = match a {
                0x0000..=0xdfff => self.mem.borrow().lb(a),
                // On dmg the dma controller extends echo mapping through 0xffff, so 0xe000-0xffff maps back to
                // 0xc000-0xdfff (via -0x2000).
                0xe000..=0xffff => self.mem.borrow().lb(a - 0x2000),
            };
            self.mem.borrow_mut().sb(0xfe00 + i as u16, b);
        }
    }
}
