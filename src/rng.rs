// A simple Linear Congruential Generator (LCG) implementation for filling a byte slice with pseudo-random data. Same
// as the one used by POSIX drand48.
// See: https://www.man7.org/linux/man-pages/man3/lcong48.3.html

use std::sync::atomic::{AtomicU64, Ordering};

const LCG_A: u64 = 0x5DEECE66D;
const LCG_C: u64 = 0xB;
const LCG_M: u64 = 1u64 << 48;
static SEED: AtomicU64 = AtomicU64::new(0x12345678);

pub fn u8() -> u8 {
    (u48() >> 40) as u8
}

pub fn u16() -> u16 {
    (u48() >> 32) as u16
}

pub fn u32() -> u32 {
    (u48() >> 16) as u32
}

pub fn u48() -> u64 {
    let mut old = SEED.load(Ordering::Relaxed);
    loop {
        let new = (LCG_A.wrapping_mul(old).wrapping_add(LCG_C)) % LCG_M;
        match SEED.compare_exchange_weak(old, new, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return new,
            Err(v) => old = v,
        }
    }
}
