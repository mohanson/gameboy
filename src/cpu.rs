// The chip behind the NINTENDO GAME BOY: The sharp LR35902.
use super::convention::{Memory, Term};
use super::register::Flag::{C, H, N, Z};
use super::register::Register;
use std::cell::RefCell;
use std::rc::Rc;
use std::thread;
use std::time;

pub const CLOCK_FREQUENCY: u32 = 4_194_304;
pub const STEP_TIME: u32 = 16;
pub const STEP_CYCLES: u32 = (STEP_TIME as f64 / (1000_f64 / CLOCK_FREQUENCY as f64)) as u32;

// Nintendo documents describe the CPU & instructions speed in machine cycles while this document describes them in
// clock cycles. Here is the translation:
//   1 machine cycle = 4 clock cycles
//                   GB CPU Speed    NOP Instruction
// Machine Cycles    1.05MHz         1 cycle
// Clock Cycles      4.19MHz         4 cycles
//
//  0  1  2  3  4  5  6  7  8  9  a  b  c  d  e  f
const OP_CYCLES: [u32; 256] = [
    1, 3, 2, 2, 1, 1, 2, 1, 5, 2, 2, 2, 1, 1, 2, 1, // 0
    1, 3, 2, 2, 1, 1, 2, 1, 3, 2, 2, 2, 1, 1, 2, 1, // 1
    2, 3, 2, 2, 1, 1, 2, 1, 2, 2, 2, 2, 1, 1, 2, 1, // 2
    2, 3, 2, 2, 3, 3, 3, 1, 2, 2, 2, 2, 1, 1, 2, 1, // 3
    1, 1, 1, 1, 1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 2, 1, // 4
    1, 1, 1, 1, 1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 2, 1, // 5
    1, 1, 1, 1, 1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 2, 1, // 6
    2, 2, 2, 2, 2, 2, 1, 2, 1, 1, 1, 1, 1, 1, 2, 1, // 7
    1, 1, 1, 1, 1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 2, 1, // 8
    1, 1, 1, 1, 1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 2, 1, // 9
    1, 1, 1, 1, 1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 2, 1, // a
    1, 1, 1, 1, 1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 2, 1, // b
    2, 3, 3, 4, 3, 4, 2, 4, 2, 4, 3, 0, 3, 6, 2, 4, // c
    2, 3, 3, 0, 3, 4, 2, 4, 2, 4, 3, 0, 3, 0, 2, 4, // d
    3, 3, 2, 0, 0, 4, 2, 4, 4, 1, 4, 0, 0, 0, 2, 4, // e
    3, 3, 2, 1, 0, 4, 2, 4, 3, 2, 4, 1, 0, 0, 2, 4, // f
];

//  0  1  2  3  4  5  6  7  8  9  a  b  c  d  e  f
const CB_CYCLES: [u32; 256] = [
    2, 2, 2, 2, 2, 2, 4, 2, 2, 2, 2, 2, 2, 2, 4, 2, // 0
    2, 2, 2, 2, 2, 2, 4, 2, 2, 2, 2, 2, 2, 2, 4, 2, // 1
    2, 2, 2, 2, 2, 2, 4, 2, 2, 2, 2, 2, 2, 2, 4, 2, // 2
    2, 2, 2, 2, 2, 2, 4, 2, 2, 2, 2, 2, 2, 2, 4, 2, // 3
    2, 2, 2, 2, 2, 2, 3, 2, 2, 2, 2, 2, 2, 2, 3, 2, // 4
    2, 2, 2, 2, 2, 2, 3, 2, 2, 2, 2, 2, 2, 2, 3, 2, // 5
    2, 2, 2, 2, 2, 2, 3, 2, 2, 2, 2, 2, 2, 2, 3, 2, // 6
    2, 2, 2, 2, 2, 2, 3, 2, 2, 2, 2, 2, 2, 2, 3, 2, // 7
    2, 2, 2, 2, 2, 2, 4, 2, 2, 2, 2, 2, 2, 2, 4, 2, // 8
    2, 2, 2, 2, 2, 2, 4, 2, 2, 2, 2, 2, 2, 2, 4, 2, // 9
    2, 2, 2, 2, 2, 2, 4, 2, 2, 2, 2, 2, 2, 2, 4, 2, // a
    2, 2, 2, 2, 2, 2, 4, 2, 2, 2, 2, 2, 2, 2, 4, 2, // b
    2, 2, 2, 2, 2, 2, 4, 2, 2, 2, 2, 2, 2, 2, 4, 2, // c
    2, 2, 2, 2, 2, 2, 4, 2, 2, 2, 2, 2, 2, 2, 4, 2, // d
    2, 2, 2, 2, 2, 2, 4, 2, 2, 2, 2, 2, 2, 2, 4, 2, // e
    2, 2, 2, 2, 2, 2, 4, 2, 2, 2, 2, 2, 2, 2, 4, 2, // f
];

pub struct Alu {}

impl Alu {
    // Add n + Carry flag to A.
    // n = A,B,C,D,E,H,L,(HL),#
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Set if carry from bit 3.
    // C - Set if carry from bit 7.
    pub fn adc(cpu: &mut Cpu, n: u8) {
        let a = cpu.reg.a;
        let c = u8::from(cpu.reg.get_flag(C));
        let r = a.wrapping_add(n).wrapping_add(c);
        cpu.reg.set_flag(C, u16::from(a) + u16::from(n) + u16::from(c) > 0xff);
        cpu.reg.set_flag(H, (a & 0x0f) + (n & 0x0f) + (c & 0x0f) > 0x0f);
        cpu.reg.set_flag(N, false);
        cpu.reg.set_flag(Z, r == 0x00);
        cpu.reg.a = r;
    }

    // Add n to A.
    // n = A,B,C,D,E,H,L,(HL),#
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Set if carry from bit 3.
    // C - Set if carry from bit 7.
    pub fn add(cpu: &mut Cpu, n: u8) {
        let a = cpu.reg.a;
        let r = a.wrapping_add(n);
        cpu.reg.set_flag(C, u16::from(a) + u16::from(n) > 0xff);
        cpu.reg.set_flag(H, (a & 0x0f) + (n & 0x0f) > 0x0f);
        cpu.reg.set_flag(N, false);
        cpu.reg.set_flag(Z, r == 0x00);
        cpu.reg.a = r;
    }

    // Add n to HL
    // n = BC,DE,HL,SP
    //
    // Flags affected:
    // Z - Not affected.
    // N - Reset.
    // H - Set if carry from bit 11.
    // C - Set if carry from bit 15.
    pub fn add_hl(cpu: &mut Cpu, n: u16) {
        let a = cpu.reg.get_hl();
        let r = a.wrapping_add(n);
        cpu.reg.set_flag(C, a > 0xffff - n);
        cpu.reg.set_flag(H, (a & 0x0fff) + (n & 0x0fff) > 0x0fff);
        cpu.reg.set_flag(N, false);
        cpu.reg.set_hl(r);
    }

    // Add n to Stack Pointer (SP).
    // n = one byte signed immediate value (#).
    //
    // Flags affected:
    // Z - Reset.
    // N - Reset.
    // H - Set or reset according to operation.
    // C - Set or reset according to operation.
    pub fn add_sp(cpu: &mut Cpu) {
        let a = cpu.reg.sp;
        let b = i16::from(cpu.fetch_b() as i8) as u16;
        cpu.reg.set_flag(C, (a & 0x00ff) + (b & 0x00ff) > 0x00ff);
        cpu.reg.set_flag(H, (a & 0x000f) + (b & 0x000f) > 0x000f);
        cpu.reg.set_flag(N, false);
        cpu.reg.set_flag(Z, false);
        cpu.reg.sp = a.wrapping_add(b);
    }

    // Logically AND n with A, result in A.
    // n = A,B,C,D,E,H,L,(HL),#
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Set.
    // C - Reset
    pub fn and(cpu: &mut Cpu, n: u8) {
        let r = cpu.reg.a & n;
        cpu.reg.set_flag(C, false);
        cpu.reg.set_flag(H, true);
        cpu.reg.set_flag(N, false);
        cpu.reg.set_flag(Z, r == 0x00);
        cpu.reg.a = r;
    }

    // Test bit b in register r.
    // b = 0 - 7, r = A,B,C,D,E,H,L,(HL)
    //
    // Flags affected:
    // Z - Set if bit b of register r is 0.
    // N - Reset.
    // H - Set.
    // C - Not affected
    pub fn bit(cpu: &mut Cpu, a: u8, b: u8) {
        let r = a & (1 << b) == 0x00;
        cpu.reg.set_flag(H, true);
        cpu.reg.set_flag(N, false);
        cpu.reg.set_flag(Z, r);
    }

    // Complement carry flag. If C flag is set, then reset it. If C flag is reset, then set it.
    // Flags affected:
    //
    // Z - Not affected.
    // N - Reset.
    // H - Reset.
    // C - Complemented.
    pub fn ccf(cpu: &mut Cpu) {
        let v = !cpu.reg.get_flag(C);
        cpu.reg.set_flag(C, v);
        cpu.reg.set_flag(H, false);
        cpu.reg.set_flag(N, false);
    }

    // Compare A with n. This is basically an A - n subtraction instruction but the results are thrown away.
    // n = A,B,C,D,E,H,L,(HL),#
    //
    // Flags affected:
    // Z - Set if result is zero. (Set if A = n.)
    // N - Set.
    // H - Set if no borrow from bit 4.
    // C - Set for no borrow. (Set if A < n.)
    pub fn cp(cpu: &mut Cpu, n: u8) {
        let a = cpu.reg.a;
        let r = a.wrapping_sub(n);
        cpu.reg.set_flag(C, u16::from(a) < u16::from(n));
        cpu.reg.set_flag(H, (a & 0x0f) < (n & 0x0f));
        cpu.reg.set_flag(N, true);
        cpu.reg.set_flag(Z, r == 0x00);
    }

    // Complement A register. (Flip all bits.)
    //
    // Flags affected:
    // Z - Not affected.
    // N - Set.
    // H - Set.
    // C - Not affected.
    pub fn cpl(cpu: &mut Cpu) {
        cpu.reg.a = !cpu.reg.a;
        cpu.reg.set_flag(H, true);
        cpu.reg.set_flag(N, true);
    }

    // Decimal adjust register A. This instruction adjusts register A so that the correct representation of Binary
    // Coded Decimal (BCD) is obtained.
    //
    // Flags affected:
    // Z - Set if register A is zero.
    // N - Not affected.
    // H - Reset.
    // C - Set or reset according to operation
    pub fn daa(cpu: &mut Cpu) {
        let mut a = cpu.reg.a;
        let mut adjust = if cpu.reg.get_flag(C) { 0x60 } else { 0x00 };
        if cpu.reg.get_flag(H) {
            adjust |= 0x06;
        };
        if !cpu.reg.get_flag(N) {
            if a & 0x0f > 0x09 {
                adjust |= 0x06;
            };
            if a > 0x99 {
                adjust |= 0x60;
            };
            a = a.wrapping_add(adjust);
        } else {
            a = a.wrapping_sub(adjust);
        }
        cpu.reg.set_flag(C, adjust >= 0x60);
        cpu.reg.set_flag(H, false);
        cpu.reg.set_flag(Z, a == 0x00);
        cpu.reg.a = a;
    }

    // Decrement register n.
    // n = A,B,C,D,E,H,L,(HL)
    //
    // Flags affected:
    // Z - Set if reselt is zero.
    // N - Set.
    // H - Set if no borrow from bit 4.
    // C - Not affected
    pub fn dec(cpu: &mut Cpu, a: u8) -> u8 {
        let r = a.wrapping_sub(1);
        cpu.reg.set_flag(H, a.trailing_zeros() >= 4);
        cpu.reg.set_flag(N, true);
        cpu.reg.set_flag(Z, r == 0);
        r
    }

    // Increment register n.
    // n = A,B,C,D,E,H,L,(HL)
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Set if carry from bit 3.
    // C - Not affected.
    pub fn inc(cpu: &mut Cpu, a: u8) -> u8 {
        let r = a.wrapping_add(1);
        cpu.reg.set_flag(H, (a & 0x0f) + 0x01 > 0x0f);
        cpu.reg.set_flag(N, false);
        cpu.reg.set_flag(Z, r == 0x00);
        r
    }

    // Add n to current address and jump to it.
    // n = one byte signed immediate value
    pub fn jr(cpu: &mut Cpu, n: u8) {
        let n = n as i8;
        cpu.reg.pc = ((u32::from(cpu.reg.pc) as i32) + i32::from(n)) as u16;
    }

    // Logical OR n with register A, result in A.
    // n = A,B,C,D,E,H,L,(HL),#
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Reset.
    // C - Reset.
    pub fn or(cpu: &mut Cpu, n: u8) {
        let r = cpu.reg.a | n;
        cpu.reg.set_flag(C, false);
        cpu.reg.set_flag(H, false);
        cpu.reg.set_flag(N, false);
        cpu.reg.set_flag(Z, r == 0x00);
        cpu.reg.a = r;
    }

    // Reset bit b in register r.
    // b = 0 - 7, r = A,B,C,D,E,H,L,(HL)
    //
    // Flags affected:  None.
    pub fn res(a: u8, b: u8) -> u8 {
        a & !(1 << b)
    }

    // Rotate A left through Carry flag.
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Reset.
    // C - Contains old bit 7 data.
    pub fn rl(cpu: &mut Cpu, a: u8) -> u8 {
        let c = (a & 0x80) >> 7 == 0x01;
        let r = (a << 1) + u8::from(cpu.reg.get_flag(C));
        cpu.reg.set_flag(C, c);
        cpu.reg.set_flag(H, false);
        cpu.reg.set_flag(N, false);
        cpu.reg.set_flag(Z, r == 0x00);
        r
    }

    // Rotate A left. Old bit 7 to Carry flag.
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Reset.
    // C - Contains old bit 7 data.
    pub fn rlc(cpu: &mut Cpu, a: u8) -> u8 {
        let c = (a & 0x80) >> 7 == 0x01;
        let r = (a << 1) | u8::from(c);
        cpu.reg.set_flag(C, c);
        cpu.reg.set_flag(H, false);
        cpu.reg.set_flag(N, false);
        cpu.reg.set_flag(Z, r == 0x00);
        r
    }

    // Rotate A right through Carry flag.
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Reset.
    // C - Contains old bit 0 data.
    pub fn rr(cpu: &mut Cpu, a: u8) -> u8 {
        let c = a & 0x01 == 0x01;
        let r = if cpu.reg.get_flag(C) { 0x80 | (a >> 1) } else { a >> 1 };
        cpu.reg.set_flag(C, c);
        cpu.reg.set_flag(H, false);
        cpu.reg.set_flag(N, false);
        cpu.reg.set_flag(Z, r == 0x00);
        r
    }

    // Rotate A right. Old bit 0 to Carry flag.
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Reset.
    // C - Contains old bit 0 data
    pub fn rrc(cpu: &mut Cpu, a: u8) -> u8 {
        let c = a & 0x01 == 0x01;
        let r = if c { 0x80 | (a >> 1) } else { a >> 1 };
        cpu.reg.set_flag(C, c);
        cpu.reg.set_flag(H, false);
        cpu.reg.set_flag(N, false);
        cpu.reg.set_flag(Z, r == 0x00);
        r
    }

    // Subtract n + Carry flag from A.
    // n = A,B,C,D,E,H,L,(HL),#
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Set.
    // H - Set if no borrow from bit 4.
    // C - Set if no borrow.
    pub fn sbc(cpu: &mut Cpu, n: u8) {
        let a = cpu.reg.a;
        let c = u8::from(cpu.reg.get_flag(C));
        let r = a.wrapping_sub(n).wrapping_sub(c);
        cpu.reg.set_flag(C, u16::from(a) < u16::from(n) + u16::from(c));
        cpu.reg.set_flag(H, (a & 0x0f) < (n & 0x0f) + c);
        cpu.reg.set_flag(N, true);
        cpu.reg.set_flag(Z, r == 0x00);
        cpu.reg.a = r;
    }

    // Set Carry flag.
    //
    // Flags affected:
    // Z - Not affected.
    // N - Reset.
    // H - Reset.
    // C - Set.
    pub fn scf(cpu: &mut Cpu) {
        cpu.reg.set_flag(C, true);
        cpu.reg.set_flag(H, false);
        cpu.reg.set_flag(N, false);
    }

    // Set bit b in register r.
    // b = 0 - 7, r = A,B,C,D,E,H,L,(HL)
    //
    // Flags affected:  None.
    pub fn set(a: u8, b: u8) -> u8 {
        a | (1 << b)
    }

    // Shift n left into Carry. LSB of n set to 0.
    // n = A,B,C,D,E,H,L,(HL)
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Reset.
    // C - Contains old bit 7 data
    pub fn sla(cpu: &mut Cpu, a: u8) -> u8 {
        let c = (a & 0x80) >> 7 == 0x01;
        let r = a << 1;
        cpu.reg.set_flag(C, c);
        cpu.reg.set_flag(H, false);
        cpu.reg.set_flag(N, false);
        cpu.reg.set_flag(Z, r == 0x00);
        r
    }

    // Shift n right into Carry. MSB doesn't change.
    // n = A,B,C,D,E,H,L,(HL)
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Reset.
    // C - Contains old bit 0 data.
    pub fn sra(cpu: &mut Cpu, a: u8) -> u8 {
        let c = a & 0x01 == 0x01;
        let r = (a >> 1) | (a & 0x80);
        cpu.reg.set_flag(C, c);
        cpu.reg.set_flag(H, false);
        cpu.reg.set_flag(N, false);
        cpu.reg.set_flag(Z, r == 0x00);
        r
    }

    // Shift n right into Carry. MSB set to 0.
    // n = A,B,C,D,E,H,L,(HL)
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Reset.
    // C - Contains old bit 0 data.
    pub fn srl(cpu: &mut Cpu, a: u8) -> u8 {
        let c = a & 0x01 == 0x01;
        let r = a >> 1;
        cpu.reg.set_flag(C, c);
        cpu.reg.set_flag(H, false);
        cpu.reg.set_flag(N, false);
        cpu.reg.set_flag(Z, r == 0x00);
        r
    }

    // Subtract n from A.
    // n = A,B,C,D,E,H,L,(HL),#
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Set.
    // H - Set if no borrow from bit 4.
    // C - Set if no borrow
    pub fn sub(cpu: &mut Cpu, n: u8) {
        let a = cpu.reg.a;
        let r = a.wrapping_sub(n);
        cpu.reg.set_flag(C, u16::from(a) < u16::from(n));
        cpu.reg.set_flag(H, (a & 0x0f) < (n & 0x0f));
        cpu.reg.set_flag(N, true);
        cpu.reg.set_flag(Z, r == 0x00);
        cpu.reg.a = r;
    }

    // Swap upper & lower nibles of n.
    // n = A,B,C,D,E,H,L,(HL)
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Reset.
    // C - Reset.
    pub fn swap(cpu: &mut Cpu, a: u8) -> u8 {
        cpu.reg.set_flag(C, false);
        cpu.reg.set_flag(H, false);
        cpu.reg.set_flag(N, false);
        cpu.reg.set_flag(Z, a == 0x00);
        (a >> 4) | (a << 4)
    }

    // Logical exclusive OR n with register A, result in A.
    // n = A,B,C,D,E,H,L,(HL),#
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Reset.
    // C - Reset.
    pub fn xor(cpu: &mut Cpu, n: u8) {
        let r = cpu.reg.a ^ n;
        cpu.reg.set_flag(C, false);
        cpu.reg.set_flag(H, false);
        cpu.reg.set_flag(N, false);
        cpu.reg.set_flag(Z, r == 0x00);
        cpu.reg.a = r;
    }
}

pub struct Cpu {
    pub reg: Register,
    pub mem: Rc<RefCell<dyn Memory>>,
    // Interrupt master enable flag, which controls whether the CPU will respond to interrupts.
    // 0: Disable.
    // 1: Enabled.
    // 2: Pending (EI executed; becomes 1 at the start of the next instruction).
    pub ime: u8,
    pub low: bool, // Low power mode.
}

// The GameBoy CPU is based on a subset of the Z80 microprocessor. A summary of these commands is given below.
// If 'Flags affected' is not given for a command then none are affected.
impl Cpu {
    fn fetch_b(&mut self) -> u8 {
        let v = self.mem.borrow().lb(self.reg.pc);
        self.reg.pc += 1;
        v
    }

    fn fetch_h(&mut self) -> u16 {
        let v = self.mem.borrow().lh(self.reg.pc);
        self.reg.pc += 2;
        v
    }

    fn stack_add(&mut self, v: u16) {
        self.reg.sp -= 2;
        self.mem.borrow_mut().sh(self.reg.sp, v);
    }

    fn stack_pop(&mut self) -> u16 {
        let r = self.mem.borrow().lh(self.reg.sp);
        self.reg.sp += 2;
        r
    }
}

impl Cpu {
    pub fn power_up(term: Term, mem: Rc<RefCell<dyn Memory>>) -> Self {
        Self { reg: Register::power_up(term), mem, ime: 1, low: false }
    }

    // The IME (interrupt master enable) flag is reset by DI and prohibits all interrupts. It is set by EI and
    // acknowledges the interrupt setting by the IE register.
    // 1. When an interrupt is generated, the IF flag will be set.
    // 2. If the IME flag is set & the corresponding IE flag is set, the following 3 steps are performed.
    // 3. Reset the IME flag and prevent all interrupts.
    // 4. The PC (program counter) is pushed onto the stack.
    // 5. Jump to the starting address of the interrupt.
    fn hi(&mut self) -> u32 {
        if !self.low && self.ime != 1 {
            return 0;
        }
        let intf = self.mem.borrow().lb(0xff0f);
        let inte = self.mem.borrow().lb(0xffff);
        let ii = intf & inte;
        if ii == 0x00 {
            return 0;
        }
        self.low = false;
        if self.ime != 1 {
            return 0;
        }
        self.ime = 0;

        // Consumer an interrupter, the rest is written back to the register
        let n = ii.trailing_zeros();
        let intf = intf & !(1 << n);
        self.mem.borrow_mut().sb(0xff0f, intf);

        self.stack_add(self.reg.pc);
        // Set the PC to correspond interrupt process program:
        // V-Blank: 0x40
        // LCD: 0x48
        // TIMER: 0x50
        // JOYPAD: 0x60
        // Serial: 0x58
        self.reg.pc = 0x0040 | ((n as u16) << 3);
        5
    }

    fn ex(&mut self) -> u32 {
        let opcode = self.fetch_b();
        let mut cbcode: u8 = 0;
        match opcode {
            0x00 => {}
            0x01 => {
                let h = self.fetch_h();
                self.reg.set_bc(h);
            }
            0x02 => self.mem.borrow_mut().sb(self.reg.get_bc(), self.reg.a),
            0x03 => self.reg.set_bc(self.reg.get_bc().wrapping_add(1)),
            0x04 => self.reg.b = Alu::inc(self, self.reg.b),
            0x05 => self.reg.b = Alu::dec(self, self.reg.b),
            0x06 => self.reg.b = self.fetch_b(),
            0x07 => {
                self.reg.a = Alu::rlc(self, self.reg.a);
                self.reg.set_flag(Z, false);
            }
            0x08 => {
                let h = self.fetch_h();
                self.mem.borrow_mut().sh(h, self.reg.sp);
            }
            0x09 => Alu::add_hl(self, self.reg.get_bc()),
            0x0a => self.reg.a = self.mem.borrow().lb(self.reg.get_bc()),
            0x0b => self.reg.set_bc(self.reg.get_bc().wrapping_sub(1)),
            0x0c => self.reg.c = Alu::inc(self, self.reg.c),
            0x0d => self.reg.c = Alu::dec(self, self.reg.c),
            0x0e => self.reg.c = self.fetch_b(),
            0x0f => {
                self.reg.a = Alu::rrc(self, self.reg.a);
                self.reg.set_flag(Z, false);
            }
            0x10 => {
                assert!(self.fetch_b() == 0x00);
            }
            0x11 => {
                let h = self.fetch_h();
                self.reg.set_de(h);
            }
            0x12 => self.mem.borrow_mut().sb(self.reg.get_de(), self.reg.a),
            0x13 => self.reg.set_de(self.reg.get_de().wrapping_add(1)),
            0x14 => self.reg.d = Alu::inc(self, self.reg.d),
            0x15 => self.reg.d = Alu::dec(self, self.reg.d),
            0x16 => self.reg.d = self.fetch_b(),
            0x17 => {
                self.reg.a = Alu::rl(self, self.reg.a);
                self.reg.set_flag(Z, false);
            }
            0x18 => {
                let b = self.fetch_b();
                Alu::jr(self, b);
            }
            0x19 => Alu::add_hl(self, self.reg.get_de()),
            0x1a => self.reg.a = self.mem.borrow().lb(self.reg.get_de()),
            0x1b => self.reg.set_de(self.reg.get_de().wrapping_sub(1)),
            0x1c => self.reg.e = Alu::inc(self, self.reg.e),
            0x1d => self.reg.e = Alu::dec(self, self.reg.e),
            0x1e => self.reg.e = self.fetch_b(),
            0x1f => {
                self.reg.a = Alu::rr(self, self.reg.a);
                self.reg.set_flag(Z, false);
            }
            0x20 => {
                let b = self.fetch_b();
                if !self.reg.get_flag(Z) {
                    Alu::jr(self, b);
                }
            }
            0x21 => {
                let h = self.fetch_h();
                self.reg.set_hl(h);
            }
            0x22 => {
                let h = self.reg.get_hl();
                self.mem.borrow_mut().sb(h, self.reg.a);
                self.reg.set_hl(h.wrapping_add(1));
            }
            0x23 => self.reg.set_hl(self.reg.get_hl().wrapping_add(1)),
            0x24 => self.reg.h = Alu::inc(self, self.reg.h),
            0x25 => self.reg.h = Alu::dec(self, self.reg.h),
            0x26 => self.reg.h = self.fetch_b(),
            0x27 => Alu::daa(self),
            0x28 => {
                let b = self.fetch_b();
                if self.reg.get_flag(Z) {
                    Alu::jr(self, b);
                }
            }
            0x29 => Alu::add_hl(self, self.reg.get_hl()),
            0x2a => {
                let h = self.reg.get_hl();
                self.reg.a = self.mem.borrow().lb(h);
                self.reg.set_hl(h.wrapping_add(1));
            }
            0x2b => self.reg.set_hl(self.reg.get_hl().wrapping_sub(1)),
            0x2c => self.reg.l = Alu::inc(self, self.reg.l),
            0x2d => self.reg.l = Alu::dec(self, self.reg.l),
            0x2e => self.reg.l = self.fetch_b(),
            0x2f => Alu::cpl(self),
            0x30 => {
                let b = self.fetch_b();
                if !self.reg.get_flag(C) {
                    Alu::jr(self, b);
                }
            }
            0x31 => self.reg.sp = self.fetch_h(),
            0x32 => {
                let h = self.reg.get_hl();
                self.mem.borrow_mut().sb(h, self.reg.a);
                self.reg.set_hl(h.wrapping_sub(1));
            }
            0x33 => self.reg.sp = self.reg.sp.wrapping_add(1),
            0x34 => {
                let h = self.reg.get_hl();
                let b = self.mem.borrow().lb(h);
                let b = Alu::inc(self, b);
                self.mem.borrow_mut().sb(h, b);
            }
            0x35 => {
                let h = self.reg.get_hl();
                let b = self.mem.borrow().lb(h);
                let b = Alu::dec(self, b);
                self.mem.borrow_mut().sb(h, b);
            }
            0x36 => {
                let h = self.reg.get_hl();
                let b = self.fetch_b();
                self.mem.borrow_mut().sb(h, b);
            }
            0x37 => Alu::scf(self),
            0x38 => {
                let b = self.fetch_b();
                if self.reg.get_flag(C) {
                    Alu::jr(self, b);
                }
            }
            0x39 => Alu::add_hl(self, self.reg.sp),
            0x3a => {
                let h = self.reg.get_hl();
                self.reg.a = self.mem.borrow().lb(h);
                self.reg.set_hl(h.wrapping_sub(1));
            }
            0x3b => self.reg.sp = self.reg.sp.wrapping_sub(1),
            0x3c => self.reg.a = Alu::inc(self, self.reg.a),
            0x3d => self.reg.a = Alu::dec(self, self.reg.a),
            0x3e => self.reg.a = self.fetch_b(),
            0x3f => Alu::ccf(self),
            0x40 => {}
            0x41 => self.reg.b = self.reg.c,
            0x42 => self.reg.b = self.reg.d,
            0x43 => self.reg.b = self.reg.e,
            0x44 => self.reg.b = self.reg.h,
            0x45 => self.reg.b = self.reg.l,
            0x46 => self.reg.b = self.mem.borrow().lb(self.reg.get_hl()),
            0x47 => self.reg.b = self.reg.a,
            0x48 => self.reg.c = self.reg.b,
            0x49 => {}
            0x4a => self.reg.c = self.reg.d,
            0x4b => self.reg.c = self.reg.e,
            0x4c => self.reg.c = self.reg.h,
            0x4d => self.reg.c = self.reg.l,
            0x4e => self.reg.c = self.mem.borrow().lb(self.reg.get_hl()),
            0x4f => self.reg.c = self.reg.a,
            0x50 => self.reg.d = self.reg.b,
            0x51 => self.reg.d = self.reg.c,
            0x52 => {}
            0x53 => self.reg.d = self.reg.e,
            0x54 => self.reg.d = self.reg.h,
            0x55 => self.reg.d = self.reg.l,
            0x56 => self.reg.d = self.mem.borrow().lb(self.reg.get_hl()),
            0x57 => self.reg.d = self.reg.a,
            0x58 => self.reg.e = self.reg.b,
            0x59 => self.reg.e = self.reg.c,
            0x5a => self.reg.e = self.reg.d,
            0x5b => {}
            0x5c => self.reg.e = self.reg.h,
            0x5d => self.reg.e = self.reg.l,
            0x5e => self.reg.e = self.mem.borrow().lb(self.reg.get_hl()),
            0x5f => self.reg.e = self.reg.a,
            0x60 => self.reg.h = self.reg.b,
            0x61 => self.reg.h = self.reg.c,
            0x62 => self.reg.h = self.reg.d,
            0x63 => self.reg.h = self.reg.e,
            0x64 => {}
            0x65 => self.reg.h = self.reg.l,
            0x66 => self.reg.h = self.mem.borrow().lb(self.reg.get_hl()),
            0x67 => self.reg.h = self.reg.a,
            0x68 => self.reg.l = self.reg.b,
            0x69 => self.reg.l = self.reg.c,
            0x6a => self.reg.l = self.reg.d,
            0x6b => self.reg.l = self.reg.e,
            0x6c => self.reg.l = self.reg.h,
            0x6d => {}
            0x6e => self.reg.l = self.mem.borrow().lb(self.reg.get_hl()),
            0x6f => self.reg.l = self.reg.a,
            0x70 => self.mem.borrow_mut().sb(self.reg.get_hl(), self.reg.b),
            0x71 => self.mem.borrow_mut().sb(self.reg.get_hl(), self.reg.c),
            0x72 => self.mem.borrow_mut().sb(self.reg.get_hl(), self.reg.d),
            0x73 => self.mem.borrow_mut().sb(self.reg.get_hl(), self.reg.e),
            0x74 => self.mem.borrow_mut().sb(self.reg.get_hl(), self.reg.h),
            0x75 => self.mem.borrow_mut().sb(self.reg.get_hl(), self.reg.l),
            0x76 => self.low = true,
            0x77 => self.mem.borrow_mut().sb(self.reg.get_hl(), self.reg.a),
            0x78 => self.reg.a = self.reg.b,
            0x79 => self.reg.a = self.reg.c,
            0x7a => self.reg.a = self.reg.d,
            0x7b => self.reg.a = self.reg.e,
            0x7c => self.reg.a = self.reg.h,
            0x7d => self.reg.a = self.reg.l,
            0x7e => self.reg.a = self.mem.borrow().lb(self.reg.get_hl()),
            0x7f => {}
            0x80 => Alu::add(self, self.reg.b),
            0x81 => Alu::add(self, self.reg.c),
            0x82 => Alu::add(self, self.reg.d),
            0x83 => Alu::add(self, self.reg.e),
            0x84 => Alu::add(self, self.reg.h),
            0x85 => Alu::add(self, self.reg.l),
            0x86 => {
                let b = self.mem.borrow().lb(self.reg.get_hl());
                Alu::add(self, b);
            }
            0x87 => Alu::add(self, self.reg.a),
            0x88 => Alu::adc(self, self.reg.b),
            0x89 => Alu::adc(self, self.reg.c),
            0x8a => Alu::adc(self, self.reg.d),
            0x8b => Alu::adc(self, self.reg.e),
            0x8c => Alu::adc(self, self.reg.h),
            0x8d => Alu::adc(self, self.reg.l),
            0x8e => {
                let b = self.mem.borrow().lb(self.reg.get_hl());
                Alu::adc(self, b);
            }
            0x8f => Alu::adc(self, self.reg.a),
            0x90 => Alu::sub(self, self.reg.b),
            0x91 => Alu::sub(self, self.reg.c),
            0x92 => Alu::sub(self, self.reg.d),
            0x93 => Alu::sub(self, self.reg.e),
            0x94 => Alu::sub(self, self.reg.h),
            0x95 => Alu::sub(self, self.reg.l),
            0x96 => {
                let b = self.mem.borrow().lb(self.reg.get_hl());
                Alu::sub(self, b);
            }
            0x97 => Alu::sub(self, self.reg.a),
            0x98 => Alu::sbc(self, self.reg.b),
            0x99 => Alu::sbc(self, self.reg.c),
            0x9a => Alu::sbc(self, self.reg.d),
            0x9b => Alu::sbc(self, self.reg.e),
            0x9c => Alu::sbc(self, self.reg.h),
            0x9d => Alu::sbc(self, self.reg.l),
            0x9e => {
                let b = self.mem.borrow().lb(self.reg.get_hl());
                Alu::sbc(self, b);
            }
            0x9f => Alu::sbc(self, self.reg.a),
            0xa0 => Alu::and(self, self.reg.b),
            0xa1 => Alu::and(self, self.reg.c),
            0xa2 => Alu::and(self, self.reg.d),
            0xa3 => Alu::and(self, self.reg.e),
            0xa4 => Alu::and(self, self.reg.h),
            0xa5 => Alu::and(self, self.reg.l),
            0xa6 => {
                let b = self.mem.borrow().lb(self.reg.get_hl());
                Alu::and(self, b);
            }
            0xa7 => Alu::and(self, self.reg.a),
            0xa8 => Alu::xor(self, self.reg.b),
            0xa9 => Alu::xor(self, self.reg.c),
            0xaa => Alu::xor(self, self.reg.d),
            0xab => Alu::xor(self, self.reg.e),
            0xac => Alu::xor(self, self.reg.h),
            0xad => Alu::xor(self, self.reg.l),
            0xae => {
                let b = self.mem.borrow().lb(self.reg.get_hl());
                Alu::xor(self, b);
            }
            0xaf => Alu::xor(self, self.reg.a),
            0xb0 => Alu::or(self, self.reg.b),
            0xb1 => Alu::or(self, self.reg.c),
            0xb2 => Alu::or(self, self.reg.d),
            0xb3 => Alu::or(self, self.reg.e),
            0xb4 => Alu::or(self, self.reg.h),
            0xb5 => Alu::or(self, self.reg.l),
            0xb6 => {
                let b = self.mem.borrow().lb(self.reg.get_hl());
                Alu::or(self, b);
            }
            0xb7 => Alu::or(self, self.reg.a),
            0xb8 => Alu::cp(self, self.reg.b),
            0xb9 => Alu::cp(self, self.reg.c),
            0xba => Alu::cp(self, self.reg.d),
            0xbb => Alu::cp(self, self.reg.e),
            0xbc => Alu::cp(self, self.reg.h),
            0xbd => Alu::cp(self, self.reg.l),
            0xbe => {
                let b = self.mem.borrow().lb(self.reg.get_hl());
                Alu::cp(self, b);
            }
            0xbf => Alu::cp(self, self.reg.a),
            0xc0 => {
                if !self.reg.get_flag(Z) {
                    self.reg.pc = self.stack_pop();
                }
            }
            0xc1 => {
                let h = self.stack_pop();
                self.reg.set_bc(h);
            }
            0xc2 => {
                let h = self.fetch_h();
                if !self.reg.get_flag(Z) {
                    self.reg.pc = h;
                }
            }
            0xc3 => self.reg.pc = self.fetch_h(),
            0xc4 => {
                let h = self.fetch_h();
                if !self.reg.get_flag(Z) {
                    self.stack_add(self.reg.pc);
                    self.reg.pc = h;
                }
            }
            0xc5 => self.stack_add(self.reg.get_bc()),
            0xc6 => {
                let b = self.fetch_b();
                Alu::add(self, b);
            }
            0xc7 => {
                self.stack_add(self.reg.pc);
                self.reg.pc = 0x00;
            }
            0xc8 => {
                if self.reg.get_flag(Z) {
                    self.reg.pc = self.stack_pop();
                }
            }
            0xc9 => self.reg.pc = self.stack_pop(),
            0xca => {
                let h = self.fetch_h();
                if self.reg.get_flag(Z) {
                    self.reg.pc = h;
                }
            }
            // Extended Bit Operations
            0xcb => {
                cbcode = self.fetch_b();
                match cbcode {
                    // RLC r8
                    0x00 => self.reg.b = Alu::rlc(self, self.reg.b),
                    0x01 => self.reg.c = Alu::rlc(self, self.reg.c),
                    0x02 => self.reg.d = Alu::rlc(self, self.reg.d),
                    0x03 => self.reg.e = Alu::rlc(self, self.reg.e),
                    0x04 => self.reg.h = Alu::rlc(self, self.reg.h),
                    0x05 => self.reg.l = Alu::rlc(self, self.reg.l),
                    0x06 => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::rlc(self, b);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0x07 => self.reg.a = Alu::rlc(self, self.reg.a),
                    0x08 => self.reg.b = Alu::rrc(self, self.reg.b),
                    0x09 => self.reg.c = Alu::rrc(self, self.reg.c),
                    0x0a => self.reg.d = Alu::rrc(self, self.reg.d),
                    0x0b => self.reg.e = Alu::rrc(self, self.reg.e),
                    0x0c => self.reg.h = Alu::rrc(self, self.reg.h),
                    0x0d => self.reg.l = Alu::rrc(self, self.reg.l),
                    0x0e => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::rrc(self, b);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0x0f => self.reg.a = Alu::rrc(self, self.reg.a),
                    0x10 => self.reg.b = Alu::rl(self, self.reg.b),
                    0x11 => self.reg.c = Alu::rl(self, self.reg.c),
                    0x12 => self.reg.d = Alu::rl(self, self.reg.d),
                    0x13 => self.reg.e = Alu::rl(self, self.reg.e),
                    0x14 => self.reg.h = Alu::rl(self, self.reg.h),
                    0x15 => self.reg.l = Alu::rl(self, self.reg.l),
                    0x16 => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::rl(self, b);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0x17 => self.reg.a = Alu::rl(self, self.reg.a),
                    0x18 => self.reg.b = Alu::rr(self, self.reg.b),
                    0x19 => self.reg.c = Alu::rr(self, self.reg.c),
                    0x1a => self.reg.d = Alu::rr(self, self.reg.d),
                    0x1b => self.reg.e = Alu::rr(self, self.reg.e),
                    0x1c => self.reg.h = Alu::rr(self, self.reg.h),
                    0x1d => self.reg.l = Alu::rr(self, self.reg.l),
                    0x1e => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::rr(self, b);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0x1f => self.reg.a = Alu::rr(self, self.reg.a),
                    0x20 => self.reg.b = Alu::sla(self, self.reg.b),
                    0x21 => self.reg.c = Alu::sla(self, self.reg.c),
                    0x22 => self.reg.d = Alu::sla(self, self.reg.d),
                    0x23 => self.reg.e = Alu::sla(self, self.reg.e),
                    0x24 => self.reg.h = Alu::sla(self, self.reg.h),
                    0x25 => self.reg.l = Alu::sla(self, self.reg.l),
                    0x26 => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::sla(self, b);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0x27 => self.reg.a = Alu::sla(self, self.reg.a),
                    0x28 => self.reg.b = Alu::sra(self, self.reg.b),
                    0x29 => self.reg.c = Alu::sra(self, self.reg.c),
                    0x2a => self.reg.d = Alu::sra(self, self.reg.d),
                    0x2b => self.reg.e = Alu::sra(self, self.reg.e),
                    0x2c => self.reg.h = Alu::sra(self, self.reg.h),
                    0x2d => self.reg.l = Alu::sra(self, self.reg.l),
                    0x2e => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::sra(self, b);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0x2f => self.reg.a = Alu::sra(self, self.reg.a),
                    0x30 => self.reg.b = Alu::swap(self, self.reg.b),
                    0x31 => self.reg.c = Alu::swap(self, self.reg.c),
                    0x32 => self.reg.d = Alu::swap(self, self.reg.d),
                    0x33 => self.reg.e = Alu::swap(self, self.reg.e),
                    0x34 => self.reg.h = Alu::swap(self, self.reg.h),
                    0x35 => self.reg.l = Alu::swap(self, self.reg.l),
                    0x36 => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::swap(self, b);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0x37 => self.reg.a = Alu::swap(self, self.reg.a),
                    0x38 => self.reg.b = Alu::srl(self, self.reg.b),
                    0x39 => self.reg.c = Alu::srl(self, self.reg.c),
                    0x3a => self.reg.d = Alu::srl(self, self.reg.d),
                    0x3b => self.reg.e = Alu::srl(self, self.reg.e),
                    0x3c => self.reg.h = Alu::srl(self, self.reg.h),
                    0x3d => self.reg.l = Alu::srl(self, self.reg.l),
                    0x3e => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::srl(self, b);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0x3f => self.reg.a = Alu::srl(self, self.reg.a),
                    0x40 => Alu::bit(self, self.reg.b, 0),
                    0x41 => Alu::bit(self, self.reg.c, 0),
                    0x42 => Alu::bit(self, self.reg.d, 0),
                    0x43 => Alu::bit(self, self.reg.e, 0),
                    0x44 => Alu::bit(self, self.reg.h, 0),
                    0x45 => Alu::bit(self, self.reg.l, 0),
                    0x46 => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        Alu::bit(self, b, 0);
                    }
                    0x47 => Alu::bit(self, self.reg.a, 0),
                    0x48 => Alu::bit(self, self.reg.b, 1),
                    0x49 => Alu::bit(self, self.reg.c, 1),
                    0x4a => Alu::bit(self, self.reg.d, 1),
                    0x4b => Alu::bit(self, self.reg.e, 1),
                    0x4c => Alu::bit(self, self.reg.h, 1),
                    0x4d => Alu::bit(self, self.reg.l, 1),
                    0x4e => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        Alu::bit(self, b, 1);
                    }
                    0x4f => Alu::bit(self, self.reg.a, 1),
                    0x50 => Alu::bit(self, self.reg.b, 2),
                    0x51 => Alu::bit(self, self.reg.c, 2),
                    0x52 => Alu::bit(self, self.reg.d, 2),
                    0x53 => Alu::bit(self, self.reg.e, 2),
                    0x54 => Alu::bit(self, self.reg.h, 2),
                    0x55 => Alu::bit(self, self.reg.l, 2),
                    0x56 => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        Alu::bit(self, b, 2);
                    }
                    0x57 => Alu::bit(self, self.reg.a, 2),
                    0x58 => Alu::bit(self, self.reg.b, 3),
                    0x59 => Alu::bit(self, self.reg.c, 3),
                    0x5a => Alu::bit(self, self.reg.d, 3),
                    0x5b => Alu::bit(self, self.reg.e, 3),
                    0x5c => Alu::bit(self, self.reg.h, 3),
                    0x5d => Alu::bit(self, self.reg.l, 3),
                    0x5e => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        Alu::bit(self, b, 3);
                    }
                    0x5f => Alu::bit(self, self.reg.a, 3),
                    0x60 => Alu::bit(self, self.reg.b, 4),
                    0x61 => Alu::bit(self, self.reg.c, 4),
                    0x62 => Alu::bit(self, self.reg.d, 4),
                    0x63 => Alu::bit(self, self.reg.e, 4),
                    0x64 => Alu::bit(self, self.reg.h, 4),
                    0x65 => Alu::bit(self, self.reg.l, 4),
                    0x66 => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        Alu::bit(self, b, 4);
                    }
                    0x67 => Alu::bit(self, self.reg.a, 4),
                    0x68 => Alu::bit(self, self.reg.b, 5),
                    0x69 => Alu::bit(self, self.reg.c, 5),
                    0x6a => Alu::bit(self, self.reg.d, 5),
                    0x6b => Alu::bit(self, self.reg.e, 5),
                    0x6c => Alu::bit(self, self.reg.h, 5),
                    0x6d => Alu::bit(self, self.reg.l, 5),
                    0x6e => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        Alu::bit(self, b, 5);
                    }
                    0x6f => Alu::bit(self, self.reg.a, 5),
                    0x70 => Alu::bit(self, self.reg.b, 6),
                    0x71 => Alu::bit(self, self.reg.c, 6),
                    0x72 => Alu::bit(self, self.reg.d, 6),
                    0x73 => Alu::bit(self, self.reg.e, 6),
                    0x74 => Alu::bit(self, self.reg.h, 6),
                    0x75 => Alu::bit(self, self.reg.l, 6),
                    0x76 => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        Alu::bit(self, b, 6);
                    }
                    0x77 => Alu::bit(self, self.reg.a, 6),
                    0x78 => Alu::bit(self, self.reg.b, 7),
                    0x79 => Alu::bit(self, self.reg.c, 7),
                    0x7a => Alu::bit(self, self.reg.d, 7),
                    0x7b => Alu::bit(self, self.reg.e, 7),
                    0x7c => Alu::bit(self, self.reg.h, 7),
                    0x7d => Alu::bit(self, self.reg.l, 7),
                    0x7e => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        Alu::bit(self, b, 7);
                    }
                    0x7f => Alu::bit(self, self.reg.a, 7),
                    0x80 => self.reg.b = Alu::res(self.reg.b, 0),
                    0x81 => self.reg.c = Alu::res(self.reg.c, 0),
                    0x82 => self.reg.d = Alu::res(self.reg.d, 0),
                    0x83 => self.reg.e = Alu::res(self.reg.e, 0),
                    0x84 => self.reg.h = Alu::res(self.reg.h, 0),
                    0x85 => self.reg.l = Alu::res(self.reg.l, 0),
                    0x86 => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::res(b, 0);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0x87 => self.reg.a = Alu::res(self.reg.a, 0),
                    0x88 => self.reg.b = Alu::res(self.reg.b, 1),
                    0x89 => self.reg.c = Alu::res(self.reg.c, 1),
                    0x8a => self.reg.d = Alu::res(self.reg.d, 1),
                    0x8b => self.reg.e = Alu::res(self.reg.e, 1),
                    0x8c => self.reg.h = Alu::res(self.reg.h, 1),
                    0x8d => self.reg.l = Alu::res(self.reg.l, 1),
                    0x8e => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::res(b, 1);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0x8f => self.reg.a = Alu::res(self.reg.a, 1),
                    0x90 => self.reg.b = Alu::res(self.reg.b, 2),
                    0x91 => self.reg.c = Alu::res(self.reg.c, 2),
                    0x92 => self.reg.d = Alu::res(self.reg.d, 2),
                    0x93 => self.reg.e = Alu::res(self.reg.e, 2),
                    0x94 => self.reg.h = Alu::res(self.reg.h, 2),
                    0x95 => self.reg.l = Alu::res(self.reg.l, 2),
                    0x96 => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::res(b, 2);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0x97 => self.reg.a = Alu::res(self.reg.a, 2),
                    0x98 => self.reg.b = Alu::res(self.reg.b, 3),
                    0x99 => self.reg.c = Alu::res(self.reg.c, 3),
                    0x9a => self.reg.d = Alu::res(self.reg.d, 3),
                    0x9b => self.reg.e = Alu::res(self.reg.e, 3),
                    0x9c => self.reg.h = Alu::res(self.reg.h, 3),
                    0x9d => self.reg.l = Alu::res(self.reg.l, 3),
                    0x9e => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::res(b, 3);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0x9f => self.reg.a = Alu::res(self.reg.a, 3),
                    0xa0 => self.reg.b = Alu::res(self.reg.b, 4),
                    0xa1 => self.reg.c = Alu::res(self.reg.c, 4),
                    0xa2 => self.reg.d = Alu::res(self.reg.d, 4),
                    0xa3 => self.reg.e = Alu::res(self.reg.e, 4),
                    0xa4 => self.reg.h = Alu::res(self.reg.h, 4),
                    0xa5 => self.reg.l = Alu::res(self.reg.l, 4),
                    0xa6 => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::res(b, 4);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0xa7 => self.reg.a = Alu::res(self.reg.a, 4),
                    0xa8 => self.reg.b = Alu::res(self.reg.b, 5),
                    0xa9 => self.reg.c = Alu::res(self.reg.c, 5),
                    0xaa => self.reg.d = Alu::res(self.reg.d, 5),
                    0xab => self.reg.e = Alu::res(self.reg.e, 5),
                    0xac => self.reg.h = Alu::res(self.reg.h, 5),
                    0xad => self.reg.l = Alu::res(self.reg.l, 5),
                    0xae => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::res(b, 5);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0xaf => self.reg.a = Alu::res(self.reg.a, 5),
                    0xb0 => self.reg.b = Alu::res(self.reg.b, 6),
                    0xb1 => self.reg.c = Alu::res(self.reg.c, 6),
                    0xb2 => self.reg.d = Alu::res(self.reg.d, 6),
                    0xb3 => self.reg.e = Alu::res(self.reg.e, 6),
                    0xb4 => self.reg.h = Alu::res(self.reg.h, 6),
                    0xb5 => self.reg.l = Alu::res(self.reg.l, 6),
                    0xb6 => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::res(b, 6);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0xb7 => self.reg.a = Alu::res(self.reg.a, 6),
                    0xb8 => self.reg.b = Alu::res(self.reg.b, 7),
                    0xb9 => self.reg.c = Alu::res(self.reg.c, 7),
                    0xba => self.reg.d = Alu::res(self.reg.d, 7),
                    0xbb => self.reg.e = Alu::res(self.reg.e, 7),
                    0xbc => self.reg.h = Alu::res(self.reg.h, 7),
                    0xbd => self.reg.l = Alu::res(self.reg.l, 7),
                    0xbe => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::res(b, 7);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0xbf => self.reg.a = Alu::res(self.reg.a, 7),
                    0xc0 => self.reg.b = Alu::set(self.reg.b, 0),
                    0xc1 => self.reg.c = Alu::set(self.reg.c, 0),
                    0xc2 => self.reg.d = Alu::set(self.reg.d, 0),
                    0xc3 => self.reg.e = Alu::set(self.reg.e, 0),
                    0xc4 => self.reg.h = Alu::set(self.reg.h, 0),
                    0xc5 => self.reg.l = Alu::set(self.reg.l, 0),
                    0xc6 => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::set(b, 0);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0xc7 => self.reg.a = Alu::set(self.reg.a, 0),
                    0xc8 => self.reg.b = Alu::set(self.reg.b, 1),
                    0xc9 => self.reg.c = Alu::set(self.reg.c, 1),
                    0xca => self.reg.d = Alu::set(self.reg.d, 1),
                    0xcb => self.reg.e = Alu::set(self.reg.e, 1),
                    0xcc => self.reg.h = Alu::set(self.reg.h, 1),
                    0xcd => self.reg.l = Alu::set(self.reg.l, 1),
                    0xce => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::set(b, 1);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0xcf => self.reg.a = Alu::set(self.reg.a, 1),
                    0xd0 => self.reg.b = Alu::set(self.reg.b, 2),
                    0xd1 => self.reg.c = Alu::set(self.reg.c, 2),
                    0xd2 => self.reg.d = Alu::set(self.reg.d, 2),
                    0xd3 => self.reg.e = Alu::set(self.reg.e, 2),
                    0xd4 => self.reg.h = Alu::set(self.reg.h, 2),
                    0xd5 => self.reg.l = Alu::set(self.reg.l, 2),
                    0xd6 => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::set(b, 2);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0xd7 => self.reg.a = Alu::set(self.reg.a, 2),
                    0xd8 => self.reg.b = Alu::set(self.reg.b, 3),
                    0xd9 => self.reg.c = Alu::set(self.reg.c, 3),
                    0xda => self.reg.d = Alu::set(self.reg.d, 3),
                    0xdb => self.reg.e = Alu::set(self.reg.e, 3),
                    0xdc => self.reg.h = Alu::set(self.reg.h, 3),
                    0xdd => self.reg.l = Alu::set(self.reg.l, 3),
                    0xde => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::set(b, 3);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0xdf => self.reg.a = Alu::set(self.reg.a, 3),
                    0xe0 => self.reg.b = Alu::set(self.reg.b, 4),
                    0xe1 => self.reg.c = Alu::set(self.reg.c, 4),
                    0xe2 => self.reg.d = Alu::set(self.reg.d, 4),
                    0xe3 => self.reg.e = Alu::set(self.reg.e, 4),
                    0xe4 => self.reg.h = Alu::set(self.reg.h, 4),
                    0xe5 => self.reg.l = Alu::set(self.reg.l, 4),
                    0xe6 => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::set(b, 4);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0xe7 => self.reg.a = Alu::set(self.reg.a, 4),
                    0xe8 => self.reg.b = Alu::set(self.reg.b, 5),
                    0xe9 => self.reg.c = Alu::set(self.reg.c, 5),
                    0xea => self.reg.d = Alu::set(self.reg.d, 5),
                    0xeb => self.reg.e = Alu::set(self.reg.e, 5),
                    0xec => self.reg.h = Alu::set(self.reg.h, 5),
                    0xed => self.reg.l = Alu::set(self.reg.l, 5),
                    0xee => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::set(b, 5);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0xef => self.reg.a = Alu::set(self.reg.a, 5),
                    0xf0 => self.reg.b = Alu::set(self.reg.b, 6),
                    0xf1 => self.reg.c = Alu::set(self.reg.c, 6),
                    0xf2 => self.reg.d = Alu::set(self.reg.d, 6),
                    0xf3 => self.reg.e = Alu::set(self.reg.e, 6),
                    0xf4 => self.reg.h = Alu::set(self.reg.h, 6),
                    0xf5 => self.reg.l = Alu::set(self.reg.l, 6),
                    0xf6 => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::set(b, 6);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0xf7 => self.reg.a = Alu::set(self.reg.a, 6),
                    0xf8 => self.reg.b = Alu::set(self.reg.b, 7),
                    0xf9 => self.reg.c = Alu::set(self.reg.c, 7),
                    0xfa => self.reg.d = Alu::set(self.reg.d, 7),
                    0xfb => self.reg.e = Alu::set(self.reg.e, 7),
                    0xfc => self.reg.h = Alu::set(self.reg.h, 7),
                    0xfd => self.reg.l = Alu::set(self.reg.l, 7),
                    0xfe => {
                        let h = self.reg.get_hl();
                        let b = self.mem.borrow().lb(h);
                        let b = Alu::set(b, 7);
                        self.mem.borrow_mut().sb(h, b);
                    }
                    0xff => self.reg.a = Alu::set(self.reg.a, 7),
                }
            }
            0xcc => {
                let h = self.fetch_h();
                if self.reg.get_flag(Z) {
                    self.stack_add(self.reg.pc);
                    self.reg.pc = h;
                }
            }
            0xcd => {
                let h = self.fetch_h();
                self.stack_add(self.reg.pc);
                self.reg.pc = h;
            }
            0xce => {
                let b = self.fetch_b();
                Alu::adc(self, b);
            }
            0xcf => {
                self.stack_add(self.reg.pc);
                self.reg.pc = 0x08;
            }
            0xd0 => {
                if !self.reg.get_flag(C) {
                    self.reg.pc = self.stack_pop();
                }
            }
            0xd1 => {
                let h = self.stack_pop();
                self.reg.set_de(h);
            }
            0xd2 => {
                let h = self.fetch_h();
                if !self.reg.get_flag(C) {
                    self.reg.pc = h;
                }
            }
            0xd3 => unreachable!(),
            // CALL IF
            0xd4 => {
                let h = self.fetch_h();
                if !self.reg.get_flag(C) {
                    self.stack_add(self.reg.pc);
                    self.reg.pc = h;
                }
            }
            0xd5 => self.stack_add(self.reg.get_de()),
            0xd6 => {
                let b = self.fetch_b();
                Alu::sub(self, b);
            }
            0xd7 => {
                self.stack_add(self.reg.pc);
                self.reg.pc = 0x10;
            }
            0xd8 => {
                if self.reg.get_flag(C) {
                    self.reg.pc = self.stack_pop();
                }
            }
            0xd9 => {
                self.reg.pc = self.stack_pop();
                self.ime = 1;
            }
            0xda => {
                let h = self.fetch_h();
                if self.reg.get_flag(C) {
                    self.reg.pc = h;
                }
            }
            0xdb => unreachable!(),
            0xdc => {
                let h = self.fetch_h();
                if self.reg.get_flag(C) {
                    self.stack_add(self.reg.pc);
                    self.reg.pc = h;
                }
            }
            0xdd => unreachable!(),
            0xde => {
                let b = self.fetch_b();
                Alu::sbc(self, b);
            }
            0xdf => {
                self.stack_add(self.reg.pc);
                self.reg.pc = 0x18;
            }
            0xe0 => {
                let h = 0xff00 | u16::from(self.fetch_b());
                self.mem.borrow_mut().sb(h, self.reg.a);
            }
            0xe1 => {
                let h = self.stack_pop();
                self.reg.set_hl(h);
            }
            0xe2 => self.mem.borrow_mut().sb(0xff00 | u16::from(self.reg.c), self.reg.a),
            0xe3 => unreachable!(),
            0xe4 => unreachable!(),
            0xe5 => self.stack_add(self.reg.get_hl()),
            0xe6 => {
                let b = self.fetch_b();
                Alu::and(self, b);
            }
            0xe7 => {
                self.stack_add(self.reg.pc);
                self.reg.pc = 0x20;
            }
            0xe8 => Alu::add_sp(self),
            0xe9 => self.reg.pc = self.reg.get_hl(),
            0xea => {
                let h = self.fetch_h();
                self.mem.borrow_mut().sb(h, self.reg.a);
            }
            0xeb => unreachable!(),
            0xec => unreachable!(),
            0xed => unreachable!(),
            0xee => {
                let b = self.fetch_b();
                Alu::xor(self, b);
            }
            0xef => {
                self.stack_add(self.reg.pc);
                self.reg.pc = 0x28;
            }
            0xf0 => {
                let h = 0xff00 | u16::from(self.fetch_b());
                self.reg.a = self.mem.borrow().lb(h);
            }
            0xf1 => {
                let h = self.stack_pop();
                self.reg.set_af(h);
            }
            0xf2 => self.reg.a = self.mem.borrow().lb(0xff00 | u16::from(self.reg.c)),
            0xf3 => self.ime = 0,
            0xf4 => unreachable!(),
            0xf5 => self.stack_add(self.reg.get_af()),
            0xf6 => {
                let b = self.fetch_b();
                Alu::or(self, b);
            }
            0xf7 => {
                self.stack_add(self.reg.pc);
                self.reg.pc = 0x30;
            }
            0xf8 => {
                let a = self.reg.sp;
                let b = i16::from(self.fetch_b() as i8) as u16;
                self.reg.set_flag(C, (a & 0x00ff) + (b & 0x00ff) > 0x00ff);
                self.reg.set_flag(H, (a & 0x000f) + (b & 0x000f) > 0x000f);
                self.reg.set_flag(N, false);
                self.reg.set_flag(Z, false);
                self.reg.set_hl(a.wrapping_add(b));
            }
            0xf9 => self.reg.sp = self.reg.get_hl(),
            0xfa => {
                let h = self.fetch_h();
                self.reg.a = self.mem.borrow().lb(h);
            }
            0xfb => self.ime = 2,
            0xfc => unreachable!(),
            0xfd => unreachable!(),
            0xfe => {
                let b = self.fetch_b();
                Alu::cp(self, b);
            }
            0xff => {
                self.stack_add(self.reg.pc);
                self.reg.pc = 0x38;
            }
        };
        let ecycle = match opcode {
            0x20 if !self.reg.get_flag(Z) => 0x01,
            0x28 if self.reg.get_flag(Z) => 0x01,
            0x30 if !self.reg.get_flag(C) => 0x01,
            0x38 if self.reg.get_flag(C) => 0x01,
            0xc0 if !self.reg.get_flag(Z) => 0x03,
            0xc2 if !self.reg.get_flag(Z) => 0x01,
            0xc4 if !self.reg.get_flag(Z) => 0x03,
            0xc8 if self.reg.get_flag(Z) => 0x03,
            0xca if self.reg.get_flag(Z) => 0x01,
            0xcc if self.reg.get_flag(Z) => 0x03,
            0xd0 if !self.reg.get_flag(C) => 0x03,
            0xd2 if !self.reg.get_flag(C) => 0x01,
            0xd4 if !self.reg.get_flag(C) => 0x03,
            0xd8 if self.reg.get_flag(C) => 0x03,
            0xda if self.reg.get_flag(C) => 0x01,
            0xdc if self.reg.get_flag(C) => 0x03,
            _ => 0x00,
        };
        if opcode == 0xcb { CB_CYCLES[cbcode as usize] } else { OP_CYCLES[opcode as usize] + ecycle }
    }

    pub fn next(&mut self) -> u32 {
        let mac = {
            let c = self.hi();
            if c != 0 {
                c
            } else if self.low {
                OP_CYCLES[0]
            } else {
                let c = self.ime == 2;
                let r = self.ex();
                if c {
                    self.ime = 1;
                }
                r
            }
        };
        mac * 4
    }
}

// Real time cpu provided to simulate real hardware speed.
pub struct Rtc {
    pub cpu: Cpu,
    step_cycles: u32,
    step_zero: time::Instant,
    step_flip: bool,
}

impl Rtc {
    pub fn power_up(term: Term, mem: Rc<RefCell<dyn Memory>>) -> Self {
        let cpu = Cpu::power_up(term, mem);
        Self { cpu, step_cycles: 0, step_zero: time::Instant::now(), step_flip: false }
    }

    // Function next simulates real hardware execution speed, by limiting the frequency of the function cpu.next().
    pub fn next(&mut self) -> u32 {
        if self.step_cycles > STEP_CYCLES {
            self.step_flip = true;
            self.step_cycles -= STEP_CYCLES;
            let now = time::Instant::now();
            let d = now.duration_since(self.step_zero);
            let s = u64::from(STEP_TIME.saturating_sub(d.as_millis() as u32));
            rog::debugln!("CPU: sleep {} millis", s);
            thread::sleep(time::Duration::from_millis(s));
            self.step_zero = self.step_zero.checked_add(time::Duration::from_millis(u64::from(STEP_TIME))).unwrap();

            // If now is after the just updated target frame time, reset to
            // avoid drift.
            if now.checked_duration_since(self.step_zero).is_some() {
                self.step_zero = now;
            }
        }
        let cycles = self.cpu.next();
        self.step_cycles += cycles;
        cycles
    }

    pub fn flip(&mut self) -> bool {
        let r = self.step_flip;
        if r {
            self.step_flip = false;
        }
        r
    }
}
