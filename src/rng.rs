// A simple Linear Congruential Generator (LCG) implementation for filling a byte slice with pseudo-random data. Same
// as the one used by POSIX drand48.
// See: https://www.man7.org/linux/man-pages/man3/lcong48.3.html

const LCG_A: u64 = 0x5DEECE66D;
const LCG_C: u64 = 0xB;
const LCG_M: u64 = 1u64 << 48;
static mut SEED: u64 = 0x12345678;

pub fn u8() -> u8 {
    unsafe {
        SEED = (LCG_A.wrapping_mul(SEED).wrapping_add(LCG_C)) % LCG_M;
        ((SEED >> 40) & 0xFF) as u8
    }
}

pub fn u16() -> u16 {
    u16::from_le_bytes([u8(), u8()])
}
