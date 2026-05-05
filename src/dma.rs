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
            cnt: Rc::new(RefCell::new(0x00)),
            mem,
            reg: Rc::new(RefCell::new(0xff)),
            sig: Rc::new(RefCell::new(0x00)),
        }
    }

    pub fn block(&self) -> bool {
        let cnt = self.cnt.borrow().clone();
        cnt > 0 && cnt <= 640
    }

    pub fn tick(&mut self, cycles: u32) {
        if *self.sig.borrow() != 0x00 {
            self.sig.replace(0x00);
            if *self.cnt.borrow() <= 640 {
                self.cnt.replace(652);
            }
        }
        if *self.cnt.borrow() == 0 {
            return;
        }
        let old = self.cnt.borrow().clone();
        let new = old.saturating_sub(cycles);
        self.cnt.replace(new);

        let ori = old.min(640);
        let end = new;
        if ori <= end {
            return;
        }
        let src = (640u32.saturating_sub(ori) + 3) / 4;
        let dst = (640u32.saturating_sub(end) + 3) / 4;
        let src_page = (*self.reg.borrow() as u16) << 8;
        for i in src..dst.min(160) {
            let b = self.lb(src_page | i as u16);
            self.sb(0xfe00 + i as u16, b);
        }
    }
}
