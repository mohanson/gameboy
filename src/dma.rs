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
    pub blk: Rc<RefCell<u8>>,
    pub cnt: u32,
    pub mem: Rc<RefCell<dyn Memory>>,
    pub reg: Rc<RefCell<u8>>,
    pub sig: Rc<RefCell<u8>>,
}

impl Memory for O {
    fn lb(&self, a: u16) -> u8 {
        match a {
            0x0000..=0xdfff => self.mem.borrow().lb(a),
            // On dmg the dma controller extends echo mapping through 0xffff, so 0xe000-0xffff maps back to
            // 0xc000-0xdfff (via -0x2000).
            0xe000..=0xffff => self.mem.borrow().lb(a - 0x2000),
        }
    }

    fn sb(&mut self, a: u16, v: u8) {
        self.mem.borrow_mut().sb(a, v);
    }
}

impl O {
    pub fn power_up(mem: Rc<RefCell<dyn Memory>>) -> Self {
        Self {
            blk: Rc::new(RefCell::new(0x00)),
            cnt: 0,
            mem,
            reg: Rc::new(RefCell::new(0xff)),
            sig: Rc::new(RefCell::new(0x00)),
        }
    }

    pub fn tick(&mut self, cycles: u32) {
        if *self.sig.borrow() != 0x00 {
            self.sig.replace(0x00);
            if self.cnt <= 640 {
                self.cnt = 652;
            }
        }
        if self.cnt == 0 {
            self.blk.replace(0x00);
            return;
        }

        let old = self.cnt;
        self.cnt = self.cnt.saturating_sub(cycles);
        let new = self.cnt;

        self.blk.replace(if new > 0 && new <= 640 { 0x01 } else { 0x00 });

        let top: u32 = 640;
        let sta = old.min(top);
        let end = new;
        if sta <= end {
            return;
        }
        let src = (top.saturating_sub(sta) + 3) / 4;
        let dst = (top.saturating_sub(end) + 3) / 4;
        let src_page = (*self.reg.borrow() as u16) << 8;
        for i in src..dst.min(160) {
            let b = self.lb(src_page | i as u16);
            self.sb(0xfe00 + i as u16, b);
        }
    }
}
