// The chip behind the NINTENDO GAME BOY: The sharp LR35902.
use super::convention::Term;
use super::memory::Memory;
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
    0, 3, 2, 2, 1, 1, 2, 1, 3, 2, 2, 2, 1, 1, 2, 1, // 1
    2, 3, 2, 2, 1, 1, 2, 1, 2, 2, 2, 2, 1, 1, 2, 1, // 2
    2, 3, 2, 2, 3, 3, 3, 1, 2, 2, 2, 2, 1, 1, 2, 1, // 3
    1, 1, 1, 1, 1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 2, 1, // 4
    1, 1, 1, 1, 1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 2, 1, // 5
    1, 1, 1, 1, 1, 1, 2, 1, 1, 1, 1, 1, 1, 1, 2, 1, // 6
    2, 2, 2, 2, 2, 2, 0, 2, 1, 1, 1, 1, 1, 1, 2, 1, // 7
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

pub struct Cpu {
    pub reg: Register,
    pub mem: Rc<RefCell<dyn Memory>>,
    pub halted: bool,
    pub ei: bool,
}

// The GameBoy CPU is based on a subset of the Z80 microprocessor. A summary of these commands is given below.
// If 'Flags affected' is not given for a command then none are affected.
impl Cpu {
    fn imm(&mut self) -> u8 {
        let v = self.mem.borrow().get(self.reg.pc);
        self.reg.pc += 1;
        v
    }

    fn imm_word(&mut self) -> u16 {
        let v = self.mem.borrow().get_word(self.reg.pc);
        self.reg.pc += 2;
        v
    }

    fn stack_add(&mut self, v: u16) {
        self.reg.sp -= 2;
        self.mem.borrow_mut().set_word(self.reg.sp, v);
    }

    fn stack_pop(&mut self) -> u16 {
        let r = self.mem.borrow().get_word(self.reg.sp);
        self.reg.sp += 2;
        r
    }

    // Add n to A.
    // n = A,B,C,D,E,H,L,(HL),#
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Set if carry from bit 3.
    // C - Set if carry from bit 7.
    fn alu_add(&mut self, n: u8) {
        let a = self.reg.a;
        let r = a.wrapping_add(n);
        self.reg.set_flag(C, u16::from(a) + u16::from(n) > 0xff);
        self.reg.set_flag(H, (a & 0x0f) + (n & 0x0f) > 0x0f);
        self.reg.set_flag(N, false);
        self.reg.set_flag(Z, r == 0x00);
        self.reg.a = r;
    }

    // Add n + Carry flag to A.
    // n = A,B,C,D,E,H,L,(HL),#
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Set if carry from bit 3.
    // C - Set if carry from bit 7.
    fn alu_adc(&mut self, n: u8) {
        let a = self.reg.a;
        let c = u8::from(self.reg.get_flag(C));
        let r = a.wrapping_add(n).wrapping_add(c);
        self.reg.set_flag(C, u16::from(a) + u16::from(n) + u16::from(c) > 0xff);
        self.reg.set_flag(H, (a & 0x0f) + (n & 0x0f) + (c & 0x0f) > 0x0f);
        self.reg.set_flag(N, false);
        self.reg.set_flag(Z, r == 0x00);
        self.reg.a = r;
    }

    // Subtract n from A.
    // n = A,B,C,D,E,H,L,(HL),#
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Set.
    // H - Set if no borrow from bit 4.
    // C - Set if no borrow
    fn alu_sub(&mut self, n: u8) {
        let a = self.reg.a;
        let r = a.wrapping_sub(n);
        self.reg.set_flag(C, u16::from(a) < u16::from(n));
        self.reg.set_flag(H, (a & 0x0f) < (n & 0x0f));
        self.reg.set_flag(N, true);
        self.reg.set_flag(Z, r == 0x00);
        self.reg.a = r;
    }

    // Subtract n + Carry flag from A.
    // n = A,B,C,D,E,H,L,(HL),#
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Set.
    // H - Set if no borrow from bit 4.
    // C - Set if no borrow.
    fn alu_sbc(&mut self, n: u8) {
        let a = self.reg.a;
        let c = u8::from(self.reg.get_flag(C));
        let r = a.wrapping_sub(n).wrapping_sub(c);
        self.reg.set_flag(C, u16::from(a) < u16::from(n) + u16::from(c));
        self.reg.set_flag(H, (a & 0x0f) < (n & 0x0f) + c);
        self.reg.set_flag(N, true);
        self.reg.set_flag(Z, r == 0x00);
        self.reg.a = r;
    }

    // Logically AND n with A, result in A.
    // n = A,B,C,D,E,H,L,(HL),#
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Set.
    // C - Reset
    fn alu_and(&mut self, n: u8) {
        let r = self.reg.a & n;
        self.reg.set_flag(C, false);
        self.reg.set_flag(H, true);
        self.reg.set_flag(N, false);
        self.reg.set_flag(Z, r == 0x00);
        self.reg.a = r;
    }

    // Logical OR n with register A, result in A.
    // n = A,B,C,D,E,H,L,(HL),#
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Reset.
    // C - Reset.
    fn alu_or(&mut self, n: u8) {
        let r = self.reg.a | n;
        self.reg.set_flag(C, false);
        self.reg.set_flag(H, false);
        self.reg.set_flag(N, false);
        self.reg.set_flag(Z, r == 0x00);
        self.reg.a = r;
    }

    // Logical exclusive OR n with register A, result in A.
    // n = A,B,C,D,E,H,L,(HL),#
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Reset.
    // C - Reset.
    fn alu_xor(&mut self, n: u8) {
        let r = self.reg.a ^ n;
        self.reg.set_flag(C, false);
        self.reg.set_flag(H, false);
        self.reg.set_flag(N, false);
        self.reg.set_flag(Z, r == 0x00);
        self.reg.a = r;
    }

    // Compare A with n. This is basically an A - n subtraction instruction but the results are thrown away.
    // n = A,B,C,D,E,H,L,(HL),#
    //
    // Flags affected:
    // Z - Set if result is zero. (Set if A = n.)
    // N - Set.
    // H - Set if no borrow from bit 4.
    // C - Set for no borrow. (Set if A < n.)
    fn alu_cp(&mut self, n: u8) {
        let r = self.reg.a;
        self.alu_sub(n);
        self.reg.a = r;
    }

    // Increment register n.
    // n = A,B,C,D,E,H,L,(HL)
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Set if carry from bit 3.
    // C - Not affected.
    fn alu_inc(&mut self, a: u8) -> u8 {
        let r = a.wrapping_add(1);
        self.reg.set_flag(H, (a & 0x0f) + 0x01 > 0x0f);
        self.reg.set_flag(N, false);
        self.reg.set_flag(Z, r == 0x00);
        r
    }

    // Decrement register n.
    // n = A,B,C,D,E,H,L,(HL)
    //
    // Flags affected:
    // Z - Set if reselt is zero.
    // N - Set.
    // H - Set if no borrow from bit 4.
    // C - Not affected
    fn alu_dec(&mut self, a: u8) -> u8 {
        let r = a.wrapping_sub(1);
        self.reg.set_flag(H, a.trailing_zeros() >= 4);
        self.reg.set_flag(N, true);
        self.reg.set_flag(Z, r == 0);
        r
    }

    // Add n to HL
    // n = BC,DE,HL,SP
    //
    // Flags affected:
    // Z - Not affected.
    // N - Reset.
    // H - Set if carry from bit 11.
    // C - Set if carry from bit 15.
    fn alu_add_hl(&mut self, n: u16) {
        let a = self.reg.get_hl();
        let r = a.wrapping_add(n);
        self.reg.set_flag(C, a > 0xffff - n);
        self.reg.set_flag(H, (a & 0x0fff) + (n & 0x0fff) > 0x0fff);
        self.reg.set_flag(N, false);
        self.reg.set_hl(r);
    }

    // Add n to Stack Pointer (SP).
    // n = one byte signed immediate value (#).
    //
    // Flags affected:
    // Z - Reset.
    // N - Reset.
    // H - Set or reset according to operation.
    // C - Set or reset according to operation.
    fn alu_add_sp(&mut self) {
        let a = self.reg.sp;
        let b = i16::from(self.imm() as i8) as u16;
        self.reg.set_flag(C, (a & 0x00ff) + (b & 0x00ff) > 0x00ff);
        self.reg.set_flag(H, (a & 0x000f) + (b & 0x000f) > 0x000f);
        self.reg.set_flag(N, false);
        self.reg.set_flag(Z, false);
        self.reg.sp = a.wrapping_add(b);
    }

    // Swap upper & lower nibles of n.
    // n = A,B,C,D,E,H,L,(HL)
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Reset.
    // C - Reset.
    fn alu_swap(&mut self, a: u8) -> u8 {
        self.reg.set_flag(C, false);
        self.reg.set_flag(H, false);
        self.reg.set_flag(N, false);
        self.reg.set_flag(Z, a == 0x00);
        (a >> 4) | (a << 4)
    }

    // Decimal adjust register A. This instruction adjusts register A so that the correct representation of Binary
    // Coded Decimal (BCD) is obtained.
    //
    // Flags affected:
    // Z - Set if register A is zero.
    // N - Not affected.
    // H - Reset.
    // C - Set or reset according to operation
    fn alu_daa(&mut self) {
        let mut a = self.reg.a;
        let mut adjust = if self.reg.get_flag(C) { 0x60 } else { 0x00 };
        if self.reg.get_flag(H) {
            adjust |= 0x06;
        };
        if !self.reg.get_flag(N) {
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
        self.reg.set_flag(C, adjust >= 0x60);
        self.reg.set_flag(H, false);
        self.reg.set_flag(Z, a == 0x00);
        self.reg.a = a;
    }

    // Complement A register. (Flip all bits.)
    //
    // Flags affected:
    // Z - Not affected.
    // N - Set.
    // H - Set.
    // C - Not affected.
    fn alu_cpl(&mut self) {
        self.reg.a = !self.reg.a;
        self.reg.set_flag(H, true);
        self.reg.set_flag(N, true);
    }

    // Complement carry flag. If C flag is set, then reset it. If C flag is reset, then set it.
    // Flags affected:
    //
    // Z - Not affected.
    // N - Reset.
    // H - Reset.
    // C - Complemented.
    fn alu_ccf(&mut self) {
        let v = !self.reg.get_flag(C);
        self.reg.set_flag(C, v);
        self.reg.set_flag(H, false);
        self.reg.set_flag(N, false);
    }

    // Set Carry flag.
    //
    // Flags affected:
    // Z - Not affected.
    // N - Reset.
    // H - Reset.
    // C - Set.
    fn alu_scf(&mut self) {
        self.reg.set_flag(C, true);
        self.reg.set_flag(H, false);
        self.reg.set_flag(N, false);
    }

    // Rotate A left. Old bit 7 to Carry flag.
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Reset.
    // C - Contains old bit 7 data.
    fn alu_rlc(&mut self, a: u8) -> u8 {
        let c = (a & 0x80) >> 7 == 0x01;
        let r = (a << 1) | u8::from(c);
        self.reg.set_flag(C, c);
        self.reg.set_flag(H, false);
        self.reg.set_flag(N, false);
        self.reg.set_flag(Z, r == 0x00);
        r
    }

    // Rotate A left through Carry flag.
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Reset.
    // C - Contains old bit 7 data.
    fn alu_rl(&mut self, a: u8) -> u8 {
        let c = (a & 0x80) >> 7 == 0x01;
        let r = (a << 1) + u8::from(self.reg.get_flag(C));
        self.reg.set_flag(C, c);
        self.reg.set_flag(H, false);
        self.reg.set_flag(N, false);
        self.reg.set_flag(Z, r == 0x00);
        r
    }

    // Rotate A right. Old bit 0 to Carry flag.
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Reset.
    // C - Contains old bit 0 data
    fn alu_rrc(&mut self, a: u8) -> u8 {
        let c = a & 0x01 == 0x01;
        let r = if c { 0x80 | (a >> 1) } else { a >> 1 };
        self.reg.set_flag(C, c);
        self.reg.set_flag(H, false);
        self.reg.set_flag(N, false);
        self.reg.set_flag(Z, r == 0x00);
        r
    }

    // Rotate A right through Carry flag.
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Reset.
    // C - Contains old bit 0 data.
    fn alu_rr(&mut self, a: u8) -> u8 {
        let c = a & 0x01 == 0x01;
        let r = if self.reg.get_flag(C) { 0x80 | (a >> 1) } else { a >> 1 };
        self.reg.set_flag(C, c);
        self.reg.set_flag(H, false);
        self.reg.set_flag(N, false);
        self.reg.set_flag(Z, r == 0x00);
        r
    }

    // Shift n left into Carry. LSB of n set to 0.
    // n = A,B,C,D,E,H,L,(HL)
    //
    // Flags affected:
    // Z - Set if result is zero.
    // N - Reset.
    // H - Reset.
    // C - Contains old bit 7 data
    fn alu_sla(&mut self, a: u8) -> u8 {
        let c = (a & 0x80) >> 7 == 0x01;
        let r = a << 1;
        self.reg.set_flag(C, c);
        self.reg.set_flag(H, false);
        self.reg.set_flag(N, false);
        self.reg.set_flag(Z, r == 0x00);
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
    fn alu_sra(&mut self, a: u8) -> u8 {
        let c = a & 0x01 == 0x01;
        let r = (a >> 1) | (a & 0x80);
        self.reg.set_flag(C, c);
        self.reg.set_flag(H, false);
        self.reg.set_flag(N, false);
        self.reg.set_flag(Z, r == 0x00);
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
    fn alu_srl(&mut self, a: u8) -> u8 {
        let c = a & 0x01 == 0x01;
        let r = a >> 1;
        self.reg.set_flag(C, c);
        self.reg.set_flag(H, false);
        self.reg.set_flag(N, false);
        self.reg.set_flag(Z, r == 0x00);
        r
    }

    // Test bit b in register r.
    // b = 0 - 7, r = A,B,C,D,E,H,L,(HL)
    //
    // Flags affected:
    // Z - Set if bit b of register r is 0.
    // N - Reset.
    // H - Set.
    // C - Not affected
    fn alu_bit(&mut self, a: u8, b: u8) {
        let r = a & (1 << b) == 0x00;
        self.reg.set_flag(H, true);
        self.reg.set_flag(N, false);
        self.reg.set_flag(Z, r);
    }

    // Set bit b in register r.
    // b = 0 - 7, r = A,B,C,D,E,H,L,(HL)
    //
    // Flags affected:  None.
    fn alu_set(&mut self, a: u8, b: u8) -> u8 {
        a | (1 << b)
    }

    // Reset bit b in register r.
    // b = 0 - 7, r = A,B,C,D,E,H,L,(HL)
    //
    // Flags affected:  None.
    fn alu_res(&mut self, a: u8, b: u8) -> u8 {
        a & !(1 << b)
    }

    // Add n to current address and jump to it.
    // n = one byte signed immediate value
    fn alu_jr(&mut self, n: u8) {
        let n = n as i8;
        self.reg.pc = ((u32::from(self.reg.pc) as i32) + i32::from(n)) as u16;
    }
}

impl Cpu {
    pub fn power_up(term: Term, mem: Rc<RefCell<dyn Memory>>) -> Self {
        Self { reg: Register::power_up(term), mem, halted: false, ei: true }
    }

    // The IME (interrupt master enable) flag is reset by DI and prohibits all interrupts. It is set by EI and
    // acknowledges the interrupt setting by the IE register.
    // 1. When an interrupt is generated, the IF flag will be set.
    // 2. If the IME flag is set & the corresponding IE flag is set, the following 3 steps are performed.
    // 3. Reset the IME flag and prevent all interrupts.
    // 4. The PC (program counter) is pushed onto the stack.
    // 5. Jump to the starting address of the interrupt.
    fn hi(&mut self) -> u32 {
        if !self.halted && !self.ei {
            return 0;
        }
        let intf = self.mem.borrow().get(0xff0f);
        let inte = self.mem.borrow().get(0xffff);
        let ii = intf & inte;
        if ii == 0x00 {
            return 0;
        }
        self.halted = false;
        if !self.ei {
            return 0;
        }
        self.ei = false;

        // Consumer an interrupter, the rest is written back to the register
        let n = ii.trailing_zeros();
        let intf = intf & !(1 << n);
        self.mem.borrow_mut().set(0xff0f, intf);

        self.stack_add(self.reg.pc);
        // Set the PC to correspond interrupt process program:
        // V-Blank: 0x40
        // LCD: 0x48
        // TIMER: 0x50
        // JOYPAD: 0x60
        // Serial: 0x58
        self.reg.pc = 0x0040 | ((n as u16) << 3);
        4
    }

    fn ex(&mut self) -> u32 {
        let opcode = self.imm();
        let mut cbcode: u8 = 0;
        match opcode {
            // LD r8, d8
            0x06 => self.reg.b = self.imm(),
            0x0e => self.reg.c = self.imm(),
            0x16 => self.reg.d = self.imm(),
            0x1e => self.reg.e = self.imm(),
            0x26 => self.reg.h = self.imm(),
            0x2e => self.reg.l = self.imm(),
            0x36 => {
                let a = self.reg.get_hl();
                let v = self.imm();
                self.mem.borrow_mut().set(a, v);
            }
            0x3e => self.reg.a = self.imm(),

            // LD (r16), A
            0x02 => self.mem.borrow_mut().set(self.reg.get_bc(), self.reg.a),
            0x12 => self.mem.borrow_mut().set(self.reg.get_de(), self.reg.a),

            // LD A, (r16)
            0x0a => self.reg.a = self.mem.borrow().get(self.reg.get_bc()),
            0x1a => self.reg.a = self.mem.borrow().get(self.reg.get_de()),

            // LD (HL+), A
            0x22 => {
                let a = self.reg.get_hl();
                self.mem.borrow_mut().set(a, self.reg.a);
                self.reg.set_hl(a + 1);
            }
            // LD (HL-), A
            0x32 => {
                let a = self.reg.get_hl();
                self.mem.borrow_mut().set(a, self.reg.a);
                self.reg.set_hl(a - 1);
            }
            // LD A, (HL+)
            0x2a => {
                let v = self.reg.get_hl();
                self.reg.a = self.mem.borrow().get(v);
                self.reg.set_hl(v + 1);
            }
            // LD A, (HL-)
            0x3a => {
                let v = self.reg.get_hl();
                self.reg.a = self.mem.borrow().get(v);
                self.reg.set_hl(v - 1);
            }

            // LD r8, r8
            0x40 => {}
            0x41 => self.reg.b = self.reg.c,
            0x42 => self.reg.b = self.reg.d,
            0x43 => self.reg.b = self.reg.e,
            0x44 => self.reg.b = self.reg.h,
            0x45 => self.reg.b = self.reg.l,
            0x46 => self.reg.b = self.mem.borrow().get(self.reg.get_hl()),
            0x47 => self.reg.b = self.reg.a,
            0x48 => self.reg.c = self.reg.b,
            0x49 => {}
            0x4a => self.reg.c = self.reg.d,
            0x4b => self.reg.c = self.reg.e,
            0x4c => self.reg.c = self.reg.h,
            0x4d => self.reg.c = self.reg.l,
            0x4e => self.reg.c = self.mem.borrow().get(self.reg.get_hl()),
            0x4f => self.reg.c = self.reg.a,
            0x50 => self.reg.d = self.reg.b,
            0x51 => self.reg.d = self.reg.c,
            0x52 => {}
            0x53 => self.reg.d = self.reg.e,
            0x54 => self.reg.d = self.reg.h,
            0x55 => self.reg.d = self.reg.l,
            0x56 => self.reg.d = self.mem.borrow().get(self.reg.get_hl()),
            0x57 => self.reg.d = self.reg.a,
            0x58 => self.reg.e = self.reg.b,
            0x59 => self.reg.e = self.reg.c,
            0x5a => self.reg.e = self.reg.d,
            0x5b => {}
            0x5c => self.reg.e = self.reg.h,
            0x5d => self.reg.e = self.reg.l,
            0x5e => self.reg.e = self.mem.borrow().get(self.reg.get_hl()),
            0x5f => self.reg.e = self.reg.a,
            0x60 => self.reg.h = self.reg.b,
            0x61 => self.reg.h = self.reg.c,
            0x62 => self.reg.h = self.reg.d,
            0x63 => self.reg.h = self.reg.e,
            0x64 => {}
            0x65 => self.reg.h = self.reg.l,
            0x66 => self.reg.h = self.mem.borrow().get(self.reg.get_hl()),
            0x67 => self.reg.h = self.reg.a,
            0x68 => self.reg.l = self.reg.b,
            0x69 => self.reg.l = self.reg.c,
            0x6a => self.reg.l = self.reg.d,
            0x6b => self.reg.l = self.reg.e,
            0x6c => self.reg.l = self.reg.h,
            0x6d => {}
            0x6e => self.reg.l = self.mem.borrow().get(self.reg.get_hl()),
            0x6f => self.reg.l = self.reg.a,
            0x70 => self.mem.borrow_mut().set(self.reg.get_hl(), self.reg.b),
            0x71 => self.mem.borrow_mut().set(self.reg.get_hl(), self.reg.c),
            0x72 => self.mem.borrow_mut().set(self.reg.get_hl(), self.reg.d),
            0x73 => self.mem.borrow_mut().set(self.reg.get_hl(), self.reg.e),
            0x74 => self.mem.borrow_mut().set(self.reg.get_hl(), self.reg.h),
            0x75 => self.mem.borrow_mut().set(self.reg.get_hl(), self.reg.l),
            0x77 => self.mem.borrow_mut().set(self.reg.get_hl(), self.reg.a),
            0x78 => self.reg.a = self.reg.b,
            0x79 => self.reg.a = self.reg.c,
            0x7a => self.reg.a = self.reg.d,
            0x7b => self.reg.a = self.reg.e,
            0x7c => self.reg.a = self.reg.h,
            0x7d => self.reg.a = self.reg.l,
            0x7e => self.reg.a = self.mem.borrow().get(self.reg.get_hl()),
            0x7f => {}

            // LDH (a8), A
            0xe0 => {
                let a = 0xff00 | u16::from(self.imm());
                self.mem.borrow_mut().set(a, self.reg.a);
            }
            // LDH A, (a8)
            0xf0 => {
                let a = 0xff00 | u16::from(self.imm());
                self.reg.a = self.mem.borrow().get(a);
            }

            // LD (C), A
            0xe2 => self.mem.borrow_mut().set(0xff00 | u16::from(self.reg.c), self.reg.a),
            // LD A, (C)
            0xf2 => self.reg.a = self.mem.borrow().get(0xff00 | u16::from(self.reg.c)),

            // LD (a16), A
            0xea => {
                let a = self.imm_word();
                self.mem.borrow_mut().set(a, self.reg.a);
            }
            // LD A, (a16)
            0xfa => {
                let a = self.imm_word();
                self.reg.a = self.mem.borrow().get(a);
            }

            // LD r16, d16
            0x01 | 0x11 | 0x21 | 0x31 => {
                let v = self.imm_word();
                match opcode {
                    0x01 => self.reg.set_bc(v),
                    0x11 => self.reg.set_de(v),
                    0x21 => self.reg.set_hl(v),
                    0x31 => self.reg.sp = v,
                    _ => {}
                }
            }

            // LD SP, HL
            0xf9 => self.reg.sp = self.reg.get_hl(),
            // LD SP, d8
            0xf8 => {
                let a = self.reg.sp;
                let b = i16::from(self.imm() as i8) as u16;
                self.reg.set_flag(C, (a & 0x00ff) + (b & 0x00ff) > 0x00ff);
                self.reg.set_flag(H, (a & 0x000f) + (b & 0x000f) > 0x000f);
                self.reg.set_flag(N, false);
                self.reg.set_flag(Z, false);
                self.reg.set_hl(a.wrapping_add(b));
            }
            // LD (d16), SP
            0x08 => {
                let a = self.imm_word();
                self.mem.borrow_mut().set_word(a, self.reg.sp);
            }

            // PUSH
            0xc5 => self.stack_add(self.reg.get_bc()),
            0xd5 => self.stack_add(self.reg.get_de()),
            0xe5 => self.stack_add(self.reg.get_hl()),
            0xf5 => self.stack_add(self.reg.get_af()),

            // POP
            0xc1 | 0xf1 | 0xd1 | 0xe1 => {
                let v = self.stack_pop();
                match opcode {
                    0xc1 => self.reg.set_bc(v),
                    0xd1 => self.reg.set_de(v),
                    0xe1 => self.reg.set_hl(v),
                    0xf1 => self.reg.set_af(v),
                    _ => {}
                }
            }

            // ADD A, r8/d8
            0x80 => self.alu_add(self.reg.b),
            0x81 => self.alu_add(self.reg.c),
            0x82 => self.alu_add(self.reg.d),
            0x83 => self.alu_add(self.reg.e),
            0x84 => self.alu_add(self.reg.h),
            0x85 => self.alu_add(self.reg.l),
            0x86 => {
                let v = self.mem.borrow().get(self.reg.get_hl());
                self.alu_add(v);
            }
            0x87 => self.alu_add(self.reg.a),
            0xc6 => {
                let v = self.imm();
                self.alu_add(v);
            }

            // ADC A, r8/d8
            0x88 => self.alu_adc(self.reg.b),
            0x89 => self.alu_adc(self.reg.c),
            0x8a => self.alu_adc(self.reg.d),
            0x8b => self.alu_adc(self.reg.e),
            0x8c => self.alu_adc(self.reg.h),
            0x8d => self.alu_adc(self.reg.l),
            0x8e => {
                let a = self.mem.borrow().get(self.reg.get_hl());
                self.alu_adc(a);
            }
            0x8f => self.alu_adc(self.reg.a),
            0xce => {
                let v = self.imm();
                self.alu_adc(v);
            }

            // SUB A, r8/d8
            0x90 => self.alu_sub(self.reg.b),
            0x91 => self.alu_sub(self.reg.c),
            0x92 => self.alu_sub(self.reg.d),
            0x93 => self.alu_sub(self.reg.e),
            0x94 => self.alu_sub(self.reg.h),
            0x95 => self.alu_sub(self.reg.l),
            0x96 => {
                let a = self.mem.borrow().get(self.reg.get_hl());
                self.alu_sub(a);
            }
            0x97 => self.alu_sub(self.reg.a),
            0xd6 => {
                let v = self.imm();
                self.alu_sub(v);
            }

            // SBC A, r8/d8
            0x98 => self.alu_sbc(self.reg.b),
            0x99 => self.alu_sbc(self.reg.c),
            0x9a => self.alu_sbc(self.reg.d),
            0x9b => self.alu_sbc(self.reg.e),
            0x9c => self.alu_sbc(self.reg.h),
            0x9d => self.alu_sbc(self.reg.l),
            0x9e => {
                let a = self.mem.borrow().get(self.reg.get_hl());
                self.alu_sbc(a);
            }
            0x9f => self.alu_sbc(self.reg.a),
            0xde => {
                let v = self.imm();
                self.alu_sbc(v);
            }

            // AND A, r8/d8
            0xa0 => self.alu_and(self.reg.b),
            0xa1 => self.alu_and(self.reg.c),
            0xa2 => self.alu_and(self.reg.d),
            0xa3 => self.alu_and(self.reg.e),
            0xa4 => self.alu_and(self.reg.h),
            0xa5 => self.alu_and(self.reg.l),
            0xa6 => {
                let a = self.mem.borrow().get(self.reg.get_hl());
                self.alu_and(a);
            }
            0xa7 => self.alu_and(self.reg.a),
            0xe6 => {
                let v = self.imm();
                self.alu_and(v);
            }

            // OR A, r8/d8
            0xb0 => self.alu_or(self.reg.b),
            0xb1 => self.alu_or(self.reg.c),
            0xb2 => self.alu_or(self.reg.d),
            0xb3 => self.alu_or(self.reg.e),
            0xb4 => self.alu_or(self.reg.h),
            0xb5 => self.alu_or(self.reg.l),
            0xb6 => {
                let a = self.mem.borrow().get(self.reg.get_hl());
                self.alu_or(a);
            }
            0xb7 => self.alu_or(self.reg.a),
            0xf6 => {
                let v = self.imm();
                self.alu_or(v);
            }

            // XOR A, r8/d8
            0xa8 => self.alu_xor(self.reg.b),
            0xa9 => self.alu_xor(self.reg.c),
            0xaa => self.alu_xor(self.reg.d),
            0xab => self.alu_xor(self.reg.e),
            0xac => self.alu_xor(self.reg.h),
            0xad => self.alu_xor(self.reg.l),
            0xae => {
                let a = self.mem.borrow().get(self.reg.get_hl());
                self.alu_xor(a);
            }
            0xaf => self.alu_xor(self.reg.a),
            0xee => {
                let v = self.imm();
                self.alu_xor(v);
            }

            // CP A, r8/d8
            0xb8 => self.alu_cp(self.reg.b),
            0xb9 => self.alu_cp(self.reg.c),
            0xba => self.alu_cp(self.reg.d),
            0xbb => self.alu_cp(self.reg.e),
            0xbc => self.alu_cp(self.reg.h),
            0xbd => self.alu_cp(self.reg.l),
            0xbe => {
                let a = self.mem.borrow().get(self.reg.get_hl());
                self.alu_cp(a);
            }
            0xbf => self.alu_cp(self.reg.a),
            0xfe => {
                let v = self.imm();
                self.alu_cp(v);
            }

            // INC r8
            0x04 => self.reg.b = self.alu_inc(self.reg.b),
            0x0c => self.reg.c = self.alu_inc(self.reg.c),
            0x14 => self.reg.d = self.alu_inc(self.reg.d),
            0x1c => self.reg.e = self.alu_inc(self.reg.e),
            0x24 => self.reg.h = self.alu_inc(self.reg.h),
            0x2c => self.reg.l = self.alu_inc(self.reg.l),
            0x34 => {
                let a = self.reg.get_hl();
                let v = self.mem.borrow().get(a);
                let h = self.alu_inc(v);
                self.mem.borrow_mut().set(a, h);
            }
            0x3c => self.reg.a = self.alu_inc(self.reg.a),

            // DEC r8
            0x05 => self.reg.b = self.alu_dec(self.reg.b),
            0x0d => self.reg.c = self.alu_dec(self.reg.c),
            0x15 => self.reg.d = self.alu_dec(self.reg.d),
            0x1d => self.reg.e = self.alu_dec(self.reg.e),
            0x25 => self.reg.h = self.alu_dec(self.reg.h),
            0x2d => self.reg.l = self.alu_dec(self.reg.l),
            0x35 => {
                let a = self.reg.get_hl();
                let v = self.mem.borrow().get(a);
                let h = self.alu_dec(v);
                self.mem.borrow_mut().set(a, h);
            }
            0x3d => self.reg.a = self.alu_dec(self.reg.a),

            // ADD HL, r16
            0x09 => self.alu_add_hl(self.reg.get_bc()),
            0x19 => self.alu_add_hl(self.reg.get_de()),
            0x29 => self.alu_add_hl(self.reg.get_hl()),
            0x39 => self.alu_add_hl(self.reg.sp),

            // ADD SP, d8
            0xe8 => self.alu_add_sp(),

            // INC r16
            0x03 => {
                let v = self.reg.get_bc().wrapping_add(1);
                self.reg.set_bc(v);
            }
            0x13 => {
                let v = self.reg.get_de().wrapping_add(1);
                self.reg.set_de(v);
            }
            0x23 => {
                let v = self.reg.get_hl().wrapping_add(1);
                self.reg.set_hl(v);
            }
            0x33 => {
                let v = self.reg.sp.wrapping_add(1);
                self.reg.sp = v;
            }

            // DEC r16
            0x0b => {
                let v = self.reg.get_bc().wrapping_sub(1);
                self.reg.set_bc(v);
            }
            0x1b => {
                let v = self.reg.get_de().wrapping_sub(1);
                self.reg.set_de(v);
            }
            0x2b => {
                let v = self.reg.get_hl().wrapping_sub(1);
                self.reg.set_hl(v);
            }
            0x3b => {
                let v = self.reg.sp.wrapping_sub(1);
                self.reg.sp = v;
            }

            // DAA
            0x27 => self.alu_daa(),

            // CPL
            0x2f => self.alu_cpl(),

            // CCF
            0x3f => self.alu_ccf(),

            // SCF
            0x37 => self.alu_scf(),

            // NOP
            0x00 => {}

            // HALT
            0x76 => self.halted = true,

            // STOP
            0x10 => {}

            // DI/EI
            0xf3 => self.ei = false,
            0xfb => self.ei = true,

            // RLCA
            0x07 => {
                self.reg.a = self.alu_rlc(self.reg.a);
                self.reg.set_flag(Z, false);
            }

            // RLA
            0x17 => {
                self.reg.a = self.alu_rl(self.reg.a);
                self.reg.set_flag(Z, false);
            }

            // RRCA
            0x0f => {
                self.reg.a = self.alu_rrc(self.reg.a);
                self.reg.set_flag(Z, false);
            }

            // RRA
            0x1f => {
                self.reg.a = self.alu_rr(self.reg.a);
                self.reg.set_flag(Z, false);
            }

            // JUMP
            0xc3 => self.reg.pc = self.imm_word(),
            0xe9 => self.reg.pc = self.reg.get_hl(),

            // JUMP IF
            0xc2 | 0xca | 0xd2 | 0xda => {
                let pc = self.imm_word();
                let cond = match opcode {
                    0xc2 => !self.reg.get_flag(Z),
                    0xca => self.reg.get_flag(Z),
                    0xd2 => !self.reg.get_flag(C),
                    0xda => self.reg.get_flag(C),
                    _ => panic!(""),
                };
                if cond {
                    self.reg.pc = pc;
                }
            }

            // JR
            0x18 => {
                let n = self.imm();
                self.alu_jr(n);
            }

            // JR IF
            0x20 | 0x28 | 0x30 | 0x38 => {
                let cond = match opcode {
                    0x20 => !self.reg.get_flag(Z),
                    0x28 => self.reg.get_flag(Z),
                    0x30 => !self.reg.get_flag(C),
                    0x38 => self.reg.get_flag(C),
                    _ => panic!(""),
                };
                let n = self.imm();
                if cond {
                    self.alu_jr(n);
                }
            }

            // CALL
            0xcd => {
                let nn = self.imm_word();
                self.stack_add(self.reg.pc);
                self.reg.pc = nn;
            }

            // CALL IF
            0xc4 | 0xcc | 0xd4 | 0xdc => {
                let cond = match opcode {
                    0xc4 => !self.reg.get_flag(Z),
                    0xcc => self.reg.get_flag(Z),
                    0xd4 => !self.reg.get_flag(C),
                    0xdc => self.reg.get_flag(C),
                    _ => panic!(""),
                };
                let nn = self.imm_word();
                if cond {
                    self.stack_add(self.reg.pc);
                    self.reg.pc = nn;
                }
            }

            // RST
            0xc7 => {
                self.stack_add(self.reg.pc);
                self.reg.pc = 0x00;
            }
            0xcf => {
                self.stack_add(self.reg.pc);
                self.reg.pc = 0x08;
            }
            0xd7 => {
                self.stack_add(self.reg.pc);
                self.reg.pc = 0x10;
            }
            0xdf => {
                self.stack_add(self.reg.pc);
                self.reg.pc = 0x18;
            }
            0xe7 => {
                self.stack_add(self.reg.pc);
                self.reg.pc = 0x20;
            }
            0xef => {
                self.stack_add(self.reg.pc);
                self.reg.pc = 0x28;
            }
            0xf7 => {
                self.stack_add(self.reg.pc);
                self.reg.pc = 0x30;
            }
            0xff => {
                self.stack_add(self.reg.pc);
                self.reg.pc = 0x38;
            }

            // RET
            0xc9 => self.reg.pc = self.stack_pop(),

            // RET IF
            0xc0 | 0xc8 | 0xd0 | 0xd8 => {
                let cond = match opcode {
                    0xc0 => !self.reg.get_flag(Z),
                    0xc8 => self.reg.get_flag(Z),
                    0xd0 => !self.reg.get_flag(C),
                    0xd8 => self.reg.get_flag(C),
                    _ => panic!(""),
                };
                if cond {
                    self.reg.pc = self.stack_pop();
                }
            }

            // RETI
            0xd9 => {
                self.reg.pc = self.stack_pop();
                self.ei = true;
            }

            // Extended Bit Operations
            0xcb => {
                cbcode = self.mem.borrow().get(self.reg.pc);
                self.reg.pc += 1;
                match cbcode {
                    // RLC r8
                    0x00 => self.reg.b = self.alu_rlc(self.reg.b),
                    0x01 => self.reg.c = self.alu_rlc(self.reg.c),
                    0x02 => self.reg.d = self.alu_rlc(self.reg.d),
                    0x03 => self.reg.e = self.alu_rlc(self.reg.e),
                    0x04 => self.reg.h = self.alu_rlc(self.reg.h),
                    0x05 => self.reg.l = self.alu_rlc(self.reg.l),
                    0x06 => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_rlc(v);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0x07 => self.reg.a = self.alu_rlc(self.reg.a),

                    // RRC r8
                    0x08 => self.reg.b = self.alu_rrc(self.reg.b),
                    0x09 => self.reg.c = self.alu_rrc(self.reg.c),
                    0x0a => self.reg.d = self.alu_rrc(self.reg.d),
                    0x0b => self.reg.e = self.alu_rrc(self.reg.e),
                    0x0c => self.reg.h = self.alu_rrc(self.reg.h),
                    0x0d => self.reg.l = self.alu_rrc(self.reg.l),
                    0x0e => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_rrc(v);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0x0f => self.reg.a = self.alu_rrc(self.reg.a),

                    // RL r8
                    0x10 => self.reg.b = self.alu_rl(self.reg.b),
                    0x11 => self.reg.c = self.alu_rl(self.reg.c),
                    0x12 => self.reg.d = self.alu_rl(self.reg.d),
                    0x13 => self.reg.e = self.alu_rl(self.reg.e),
                    0x14 => self.reg.h = self.alu_rl(self.reg.h),
                    0x15 => self.reg.l = self.alu_rl(self.reg.l),
                    0x16 => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_rl(v);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0x17 => self.reg.a = self.alu_rl(self.reg.a),

                    // RR r8
                    0x18 => self.reg.b = self.alu_rr(self.reg.b),
                    0x19 => self.reg.c = self.alu_rr(self.reg.c),
                    0x1a => self.reg.d = self.alu_rr(self.reg.d),
                    0x1b => self.reg.e = self.alu_rr(self.reg.e),
                    0x1c => self.reg.h = self.alu_rr(self.reg.h),
                    0x1d => self.reg.l = self.alu_rr(self.reg.l),
                    0x1e => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_rr(v);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0x1f => self.reg.a = self.alu_rr(self.reg.a),

                    // SLA r8
                    0x20 => self.reg.b = self.alu_sla(self.reg.b),
                    0x21 => self.reg.c = self.alu_sla(self.reg.c),
                    0x22 => self.reg.d = self.alu_sla(self.reg.d),
                    0x23 => self.reg.e = self.alu_sla(self.reg.e),
                    0x24 => self.reg.h = self.alu_sla(self.reg.h),
                    0x25 => self.reg.l = self.alu_sla(self.reg.l),
                    0x26 => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_sla(v);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0x27 => self.reg.a = self.alu_sla(self.reg.a),

                    // SRA r8
                    0x28 => self.reg.b = self.alu_sra(self.reg.b),
                    0x29 => self.reg.c = self.alu_sra(self.reg.c),
                    0x2a => self.reg.d = self.alu_sra(self.reg.d),
                    0x2b => self.reg.e = self.alu_sra(self.reg.e),
                    0x2c => self.reg.h = self.alu_sra(self.reg.h),
                    0x2d => self.reg.l = self.alu_sra(self.reg.l),
                    0x2e => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_sra(v);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0x2f => self.reg.a = self.alu_sra(self.reg.a),

                    // SWAP r8
                    0x30 => self.reg.b = self.alu_swap(self.reg.b),
                    0x31 => self.reg.c = self.alu_swap(self.reg.c),
                    0x32 => self.reg.d = self.alu_swap(self.reg.d),
                    0x33 => self.reg.e = self.alu_swap(self.reg.e),
                    0x34 => self.reg.h = self.alu_swap(self.reg.h),
                    0x35 => self.reg.l = self.alu_swap(self.reg.l),
                    0x36 => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_swap(v);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0x37 => self.reg.a = self.alu_swap(self.reg.a),

                    // SRL r8
                    0x38 => self.reg.b = self.alu_srl(self.reg.b),
                    0x39 => self.reg.c = self.alu_srl(self.reg.c),
                    0x3a => self.reg.d = self.alu_srl(self.reg.d),
                    0x3b => self.reg.e = self.alu_srl(self.reg.e),
                    0x3c => self.reg.h = self.alu_srl(self.reg.h),
                    0x3d => self.reg.l = self.alu_srl(self.reg.l),
                    0x3e => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_srl(v);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0x3f => self.reg.a = self.alu_srl(self.reg.a),

                    // BIT b, r8
                    0x40 => self.alu_bit(self.reg.b, 0),
                    0x41 => self.alu_bit(self.reg.c, 0),
                    0x42 => self.alu_bit(self.reg.d, 0),
                    0x43 => self.alu_bit(self.reg.e, 0),
                    0x44 => self.alu_bit(self.reg.h, 0),
                    0x45 => self.alu_bit(self.reg.l, 0),
                    0x46 => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        self.alu_bit(v, 0);
                    }
                    0x47 => self.alu_bit(self.reg.a, 0),
                    0x48 => self.alu_bit(self.reg.b, 1),
                    0x49 => self.alu_bit(self.reg.c, 1),
                    0x4a => self.alu_bit(self.reg.d, 1),
                    0x4b => self.alu_bit(self.reg.e, 1),
                    0x4c => self.alu_bit(self.reg.h, 1),
                    0x4d => self.alu_bit(self.reg.l, 1),
                    0x4e => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        self.alu_bit(v, 1);
                    }
                    0x4f => self.alu_bit(self.reg.a, 1),
                    0x50 => self.alu_bit(self.reg.b, 2),
                    0x51 => self.alu_bit(self.reg.c, 2),
                    0x52 => self.alu_bit(self.reg.d, 2),
                    0x53 => self.alu_bit(self.reg.e, 2),
                    0x54 => self.alu_bit(self.reg.h, 2),
                    0x55 => self.alu_bit(self.reg.l, 2),
                    0x56 => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        self.alu_bit(v, 2);
                    }
                    0x57 => self.alu_bit(self.reg.a, 2),
                    0x58 => self.alu_bit(self.reg.b, 3),
                    0x59 => self.alu_bit(self.reg.c, 3),
                    0x5a => self.alu_bit(self.reg.d, 3),
                    0x5b => self.alu_bit(self.reg.e, 3),
                    0x5c => self.alu_bit(self.reg.h, 3),
                    0x5d => self.alu_bit(self.reg.l, 3),
                    0x5e => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        self.alu_bit(v, 3);
                    }
                    0x5f => self.alu_bit(self.reg.a, 3),
                    0x60 => self.alu_bit(self.reg.b, 4),
                    0x61 => self.alu_bit(self.reg.c, 4),
                    0x62 => self.alu_bit(self.reg.d, 4),
                    0x63 => self.alu_bit(self.reg.e, 4),
                    0x64 => self.alu_bit(self.reg.h, 4),
                    0x65 => self.alu_bit(self.reg.l, 4),
                    0x66 => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        self.alu_bit(v, 4);
                    }
                    0x67 => self.alu_bit(self.reg.a, 4),
                    0x68 => self.alu_bit(self.reg.b, 5),
                    0x69 => self.alu_bit(self.reg.c, 5),
                    0x6a => self.alu_bit(self.reg.d, 5),
                    0x6b => self.alu_bit(self.reg.e, 5),
                    0x6c => self.alu_bit(self.reg.h, 5),
                    0x6d => self.alu_bit(self.reg.l, 5),
                    0x6e => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        self.alu_bit(v, 5);
                    }
                    0x6f => self.alu_bit(self.reg.a, 5),
                    0x70 => self.alu_bit(self.reg.b, 6),
                    0x71 => self.alu_bit(self.reg.c, 6),
                    0x72 => self.alu_bit(self.reg.d, 6),
                    0x73 => self.alu_bit(self.reg.e, 6),
                    0x74 => self.alu_bit(self.reg.h, 6),
                    0x75 => self.alu_bit(self.reg.l, 6),
                    0x76 => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        self.alu_bit(v, 6);
                    }
                    0x77 => self.alu_bit(self.reg.a, 6),
                    0x78 => self.alu_bit(self.reg.b, 7),
                    0x79 => self.alu_bit(self.reg.c, 7),
                    0x7a => self.alu_bit(self.reg.d, 7),
                    0x7b => self.alu_bit(self.reg.e, 7),
                    0x7c => self.alu_bit(self.reg.h, 7),
                    0x7d => self.alu_bit(self.reg.l, 7),
                    0x7e => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        self.alu_bit(v, 7);
                    }
                    0x7f => self.alu_bit(self.reg.a, 7),

                    // RES b, r8
                    0x80 => self.reg.b = self.alu_res(self.reg.b, 0),
                    0x81 => self.reg.c = self.alu_res(self.reg.c, 0),
                    0x82 => self.reg.d = self.alu_res(self.reg.d, 0),
                    0x83 => self.reg.e = self.alu_res(self.reg.e, 0),
                    0x84 => self.reg.h = self.alu_res(self.reg.h, 0),
                    0x85 => self.reg.l = self.alu_res(self.reg.l, 0),
                    0x86 => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_res(v, 0);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0x87 => self.reg.a = self.alu_res(self.reg.a, 0),
                    0x88 => self.reg.b = self.alu_res(self.reg.b, 1),
                    0x89 => self.reg.c = self.alu_res(self.reg.c, 1),
                    0x8a => self.reg.d = self.alu_res(self.reg.d, 1),
                    0x8b => self.reg.e = self.alu_res(self.reg.e, 1),
                    0x8c => self.reg.h = self.alu_res(self.reg.h, 1),
                    0x8d => self.reg.l = self.alu_res(self.reg.l, 1),
                    0x8e => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_res(v, 1);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0x8f => self.reg.a = self.alu_res(self.reg.a, 1),
                    0x90 => self.reg.b = self.alu_res(self.reg.b, 2),
                    0x91 => self.reg.c = self.alu_res(self.reg.c, 2),
                    0x92 => self.reg.d = self.alu_res(self.reg.d, 2),
                    0x93 => self.reg.e = self.alu_res(self.reg.e, 2),
                    0x94 => self.reg.h = self.alu_res(self.reg.h, 2),
                    0x95 => self.reg.l = self.alu_res(self.reg.l, 2),
                    0x96 => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_res(v, 2);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0x97 => self.reg.a = self.alu_res(self.reg.a, 2),
                    0x98 => self.reg.b = self.alu_res(self.reg.b, 3),
                    0x99 => self.reg.c = self.alu_res(self.reg.c, 3),
                    0x9a => self.reg.d = self.alu_res(self.reg.d, 3),
                    0x9b => self.reg.e = self.alu_res(self.reg.e, 3),
                    0x9c => self.reg.h = self.alu_res(self.reg.h, 3),
                    0x9d => self.reg.l = self.alu_res(self.reg.l, 3),
                    0x9e => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_res(v, 3);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0x9f => self.reg.a = self.alu_res(self.reg.a, 3),
                    0xa0 => self.reg.b = self.alu_res(self.reg.b, 4),
                    0xa1 => self.reg.c = self.alu_res(self.reg.c, 4),
                    0xa2 => self.reg.d = self.alu_res(self.reg.d, 4),
                    0xa3 => self.reg.e = self.alu_res(self.reg.e, 4),
                    0xa4 => self.reg.h = self.alu_res(self.reg.h, 4),
                    0xa5 => self.reg.l = self.alu_res(self.reg.l, 4),
                    0xa6 => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_res(v, 4);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0xa7 => self.reg.a = self.alu_res(self.reg.a, 4),
                    0xa8 => self.reg.b = self.alu_res(self.reg.b, 5),
                    0xa9 => self.reg.c = self.alu_res(self.reg.c, 5),
                    0xaa => self.reg.d = self.alu_res(self.reg.d, 5),
                    0xab => self.reg.e = self.alu_res(self.reg.e, 5),
                    0xac => self.reg.h = self.alu_res(self.reg.h, 5),
                    0xad => self.reg.l = self.alu_res(self.reg.l, 5),
                    0xae => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_res(v, 5);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0xaf => self.reg.a = self.alu_res(self.reg.a, 5),
                    0xb0 => self.reg.b = self.alu_res(self.reg.b, 6),
                    0xb1 => self.reg.c = self.alu_res(self.reg.c, 6),
                    0xb2 => self.reg.d = self.alu_res(self.reg.d, 6),
                    0xb3 => self.reg.e = self.alu_res(self.reg.e, 6),
                    0xb4 => self.reg.h = self.alu_res(self.reg.h, 6),
                    0xb5 => self.reg.l = self.alu_res(self.reg.l, 6),
                    0xb6 => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_res(v, 6);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0xb7 => self.reg.a = self.alu_res(self.reg.a, 6),
                    0xb8 => self.reg.b = self.alu_res(self.reg.b, 7),
                    0xb9 => self.reg.c = self.alu_res(self.reg.c, 7),
                    0xba => self.reg.d = self.alu_res(self.reg.d, 7),
                    0xbb => self.reg.e = self.alu_res(self.reg.e, 7),
                    0xbc => self.reg.h = self.alu_res(self.reg.h, 7),
                    0xbd => self.reg.l = self.alu_res(self.reg.l, 7),
                    0xbe => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_res(v, 7);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0xbf => self.reg.a = self.alu_res(self.reg.a, 7),

                    // SET b, r8
                    0xc0 => self.reg.b = self.alu_set(self.reg.b, 0),
                    0xc1 => self.reg.c = self.alu_set(self.reg.c, 0),
                    0xc2 => self.reg.d = self.alu_set(self.reg.d, 0),
                    0xc3 => self.reg.e = self.alu_set(self.reg.e, 0),
                    0xc4 => self.reg.h = self.alu_set(self.reg.h, 0),
                    0xc5 => self.reg.l = self.alu_set(self.reg.l, 0),
                    0xc6 => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_set(v, 0);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0xc7 => self.reg.a = self.alu_set(self.reg.a, 0),
                    0xc8 => self.reg.b = self.alu_set(self.reg.b, 1),
                    0xc9 => self.reg.c = self.alu_set(self.reg.c, 1),
                    0xca => self.reg.d = self.alu_set(self.reg.d, 1),
                    0xcb => self.reg.e = self.alu_set(self.reg.e, 1),
                    0xcc => self.reg.h = self.alu_set(self.reg.h, 1),
                    0xcd => self.reg.l = self.alu_set(self.reg.l, 1),
                    0xce => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_set(v, 1);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0xcf => self.reg.a = self.alu_set(self.reg.a, 1),
                    0xd0 => self.reg.b = self.alu_set(self.reg.b, 2),
                    0xd1 => self.reg.c = self.alu_set(self.reg.c, 2),
                    0xd2 => self.reg.d = self.alu_set(self.reg.d, 2),
                    0xd3 => self.reg.e = self.alu_set(self.reg.e, 2),
                    0xd4 => self.reg.h = self.alu_set(self.reg.h, 2),
                    0xd5 => self.reg.l = self.alu_set(self.reg.l, 2),
                    0xd6 => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_set(v, 2);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0xd7 => self.reg.a = self.alu_set(self.reg.a, 2),
                    0xd8 => self.reg.b = self.alu_set(self.reg.b, 3),
                    0xd9 => self.reg.c = self.alu_set(self.reg.c, 3),
                    0xda => self.reg.d = self.alu_set(self.reg.d, 3),
                    0xdb => self.reg.e = self.alu_set(self.reg.e, 3),
                    0xdc => self.reg.h = self.alu_set(self.reg.h, 3),
                    0xdd => self.reg.l = self.alu_set(self.reg.l, 3),
                    0xde => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_set(v, 3);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0xdf => self.reg.a = self.alu_set(self.reg.a, 3),
                    0xe0 => self.reg.b = self.alu_set(self.reg.b, 4),
                    0xe1 => self.reg.c = self.alu_set(self.reg.c, 4),
                    0xe2 => self.reg.d = self.alu_set(self.reg.d, 4),
                    0xe3 => self.reg.e = self.alu_set(self.reg.e, 4),
                    0xe4 => self.reg.h = self.alu_set(self.reg.h, 4),
                    0xe5 => self.reg.l = self.alu_set(self.reg.l, 4),
                    0xe6 => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_set(v, 4);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0xe7 => self.reg.a = self.alu_set(self.reg.a, 4),
                    0xe8 => self.reg.b = self.alu_set(self.reg.b, 5),
                    0xe9 => self.reg.c = self.alu_set(self.reg.c, 5),
                    0xea => self.reg.d = self.alu_set(self.reg.d, 5),
                    0xeb => self.reg.e = self.alu_set(self.reg.e, 5),
                    0xec => self.reg.h = self.alu_set(self.reg.h, 5),
                    0xed => self.reg.l = self.alu_set(self.reg.l, 5),
                    0xee => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_set(v, 5);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0xef => self.reg.a = self.alu_set(self.reg.a, 5),
                    0xf0 => self.reg.b = self.alu_set(self.reg.b, 6),
                    0xf1 => self.reg.c = self.alu_set(self.reg.c, 6),
                    0xf2 => self.reg.d = self.alu_set(self.reg.d, 6),
                    0xf3 => self.reg.e = self.alu_set(self.reg.e, 6),
                    0xf4 => self.reg.h = self.alu_set(self.reg.h, 6),
                    0xf5 => self.reg.l = self.alu_set(self.reg.l, 6),
                    0xf6 => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_set(v, 6);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0xf7 => self.reg.a = self.alu_set(self.reg.a, 6),
                    0xf8 => self.reg.b = self.alu_set(self.reg.b, 7),
                    0xf9 => self.reg.c = self.alu_set(self.reg.c, 7),
                    0xfa => self.reg.d = self.alu_set(self.reg.d, 7),
                    0xfb => self.reg.e = self.alu_set(self.reg.e, 7),
                    0xfc => self.reg.h = self.alu_set(self.reg.h, 7),
                    0xfd => self.reg.l = self.alu_set(self.reg.l, 7),
                    0xfe => {
                        let a = self.reg.get_hl();
                        let v = self.mem.borrow().get(a);
                        let h = self.alu_set(v, 7);
                        self.mem.borrow_mut().set(a, h);
                    }
                    0xff => self.reg.a = self.alu_set(self.reg.a, 7),
                }
            }
            0xd3 => panic!("Opcode 0xd3 is not implemented"),
            0xdb => panic!("Opcode 0xdb is not implemented"),
            0xdd => panic!("Opcode 0xdd is not implemented"),
            0xe3 => panic!("Opcode 0xe3 is not implemented"),
            0xe4 => panic!("Opcode 0xd4 is not implemented"),
            0xeb => panic!("Opcode 0xeb is not implemented"),
            0xec => panic!("Opcode 0xec is not implemented"),
            0xed => panic!("Opcode 0xed is not implemented"),
            0xf4 => panic!("Opcode 0xf4 is not implemented"),
            0xfc => panic!("Opcode 0xfc is not implemented"),
            0xfd => panic!("Opcode 0xfd is not implemented"),
        };

        let ecycle = match opcode {
            0x20 | 0x30 => {
                if self.reg.get_flag(Z) {
                    0x00
                } else {
                    0x01
                }
            }
            0x28 | 0x38 => {
                if self.reg.get_flag(Z) {
                    0x01
                } else {
                    0x00
                }
            }
            0xc0 | 0xd0 => {
                if self.reg.get_flag(Z) {
                    0x00
                } else {
                    0x03
                }
            }
            0xc8 | 0xcc | 0xd8 | 0xdc => {
                if self.reg.get_flag(Z) {
                    0x03
                } else {
                    0x00
                }
            }
            0xc2 | 0xd2 => {
                if self.reg.get_flag(Z) {
                    0x00
                } else {
                    0x01
                }
            }
            0xca | 0xda => {
                if self.reg.get_flag(Z) {
                    0x01
                } else {
                    0x00
                }
            }
            0xc4 | 0xd4 => {
                if self.reg.get_flag(Z) {
                    0x00
                } else {
                    0x03
                }
            }
            _ => 0x00,
        };
        if opcode == 0xcb { CB_CYCLES[cbcode as usize] } else { OP_CYCLES[opcode as usize] + ecycle }
    }

    pub fn next(&mut self) -> u32 {
        let mac = {
            let c = self.hi();
            if c != 0 {
                c
            } else if self.halted {
                OP_CYCLES[0]
            } else {
                self.ex()
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
