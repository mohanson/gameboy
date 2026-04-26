// As the gameboys 16 bit address bus offers only limited space for ROM and RAM addressing, many games are using Memory
// Bank Controllers (MBCs) to expand the available address space by bank switching. These MBC chips are located in the
// game cartridge (ie. not in the gameboy itself).
//
// In each cartridge, the required (or preferred) MBC type should be specified in the byte at 0147h of the ROM, as
// described in the cartridge header. Several different MBC types are available.
//
// Reference:
//   - http://gbdev.gg8.se/wiki/articles/The_Cartridge_Header
//   - http://gbdev.gg8.se/wiki/articles/Memory_Bank_Controllers
use super::memory::Memory;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::SystemTime;

// These bytes define the bitmap of the Nintendo logo that is displayed when the gameboy gets turned on.
// The reason for joining is because if the pirates copy the cartridge, they must also copy Nintendo's LOGO,
// which infringes the trademark law. In the early days, the copyright law is not perfect for the determination of
// electronic data.
// The hexdump of this bitmap is:
const NINTENDO_LOGO: [u8; 48] = [
    0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B, 0x03, 0x73, 0x00, 0x83, 0x00, 0x0C, 0x00, 0x0D, 0x00, 0x08, 0x11,
    0x1F, 0x88, 0x89, 0x00, 0x0E, 0xDC, 0xCC, 0x6E, 0xE6, 0xDD, 0xDD, 0xD9, 0x99, 0xBB, 0xBB, 0x67, 0x63, 0x6E, 0x0E,
    0xEC, 0xCC, 0xDD, 0xDC, 0x99, 0x9F, 0xBB, 0xB9, 0x33, 0x3E,
];
const READABLE_TYPE: LazyLock<HashMap<u8, &str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(0x00, "ROM ONLY");
    m.insert(0x01, "MBC1");
    m.insert(0x02, "MBC1+RAM");
    m.insert(0x03, "MBC1+RAM+BATTERY");
    m.insert(0x05, "MBC2");
    m.insert(0x06, "MBC2+BATTERY");
    m.insert(0x08, "ROM+RAM");
    m.insert(0x09, "ROM+RAM+BATTERY");
    m.insert(0x0b, "MMM01");
    m.insert(0x0c, "MMM01+RAM");
    m.insert(0x0d, "MMM01+RAM+BATTERY");
    m.insert(0x0f, "MBC3+TIMER+BATTERY");
    m.insert(0x10, "MBC3+TIMER+RAM+BATTERY");
    m.insert(0x11, "MBC3");
    m.insert(0x12, "MBC3+RAM");
    m.insert(0x13, "MBC3+RAM+BATTERY");
    m.insert(0x15, "MBC4");
    m.insert(0x16, "MBC4+RAM");
    m.insert(0x17, "MBC4+RAM+BATTERY");
    m.insert(0x19, "MBC5");
    m.insert(0x1a, "MBC5+RAM");
    m.insert(0x1b, "MBC5+RAM+BATTERY");
    m.insert(0x1c, "MBC5+RUMBLE");
    m.insert(0x1d, "MBC5+RUMBLE+RAM");
    m.insert(0x1e, "MBC5+RUMBLE+RAM+BATTERY");
    m.insert(0x20, "MBC6");
    m.insert(0x22, "MBC7+SENSOR+RUMBLE+RAM+BATTERY");
    m.insert(0xfc, "POCKET CAMERA");
    m.insert(0xfd, "BANDAI TAMA5");
    m.insert(0xfe, "HuC3");
    m.insert(0xff, "HuC1+RAM+BATTERY");
    m
});
const ROM_BANK_LENGTH: usize = 1024 * 16;
const ROM_BANK_NUMBER: LazyLock<HashMap<u8, usize>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(0x00, 2);
    m.insert(0x01, 4);
    m.insert(0x02, 8);
    m.insert(0x03, 16);
    m.insert(0x04, 32);
    m.insert(0x05, 64);
    m.insert(0x06, 128);
    m.insert(0x07, 256);
    m.insert(0x08, 512);
    m.insert(0x52, 72);
    m.insert(0x53, 80);
    m.insert(0x54, 96);
    m
});
const RAM_BANK_LENGTH: usize = 1024;
const RAM_BANK_NUMBER: LazyLock<HashMap<u8, usize>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert(0x00, 0);
    m.insert(0x01, 0);
    m.insert(0x02, 8);
    m.insert(0x03, 32);
    m.insert(0x04, 128);
    m.insert(0x05, 64);
    m
});

pub trait Stable: Memory {
    fn sav(&self);
}

// This is a 32kB (256kb) ROM and occupies 0000-7FFF.
pub struct RomOnly {
    rom: Vec<u8>,
}

impl RomOnly {
    pub fn power_up(rom: Vec<u8>) -> Self {
        RomOnly { rom }
    }
}

impl Memory for RomOnly {
    fn get(&self, a: u16) -> u8 {
        self.rom[a as usize]
    }

    fn set(&mut self, _: u16, _: u8) {}
}

impl Stable for RomOnly {
    fn sav(&self) {}
}

// This is the first MBC chip for the gameboy. Any newer MBC chips are working similiar, so that is relative easy to
// upgrade a program from one MBC chip to another - or even to make it compatible to several different types of MBCs.
// Note that the memory in range 0000-7FFF is used for both reading from ROM, and for writing to the MBCs Control
// Registers.
//
// 0000-3FFF - ROM Bank 00 (Read Only)
// This area always contains the first 16KBytes of the cartridge ROM.
//
// 4000-7FFF - ROM Bank 01-7F (Read Only)
// This area may contain any of the further 16KByte banks of the ROM, allowing to address up to 125 ROM Banks
// (almost 2MByte). As described below, bank numbers 20h, 40h, and 60h cannot be used, resulting in the odd amount of
// 125 banks.
//
// A000-BFFF - RAM Bank 00-03, if any (Read/Write)
// This area is used to address external RAM in the cartridge (if any). External RAM is often battery buffered,
// allowing to store game positions or high score tables, even if the gameboy is turned off, or if the cartridge is
// removed from the gameboy. Available RAM sizes are: 2KByte (at A000-A7FF), 8KByte (at A000-BFFF), and 32KByte (in
// form of four 8K banks at A000-BFFF).
//
// 0000-1FFF - RAM Enable (Write Only)
// Before external RAM can be read or written, it must be enabled by writing to this address space. It is recommended
// to disable external RAM after accessing it, in order to protect its contents from damage during power down of the
// gameboy. Usually the following values are used:
//   00h  Disable RAM (default)
//   0Ah  Enable RAM
// Practically any value with 0Ah in the lower 4 bits enables RAM, and any other value disables RAM.
//
// 2000-3FFF - ROM Bank Number (Write Only)
// Writing to this address space selects the lower 5 bits of the ROM Bank Number (in range 01-1Fh). When 00h is written,
// the MBC translates that to bank 01h also. That doesn't harm so far, because ROM Bank 00h can be always directly
// accessed by reading from 0000-3FFF. But (when using the register below to specify the upper ROM Bank bits), the same
// happens for Bank 20h, 40h, and 60h. Any attempt to address these ROM Banks will select Bank 21h, 41h, and 61h
// instead.
//
// 4000-5FFF - RAM Bank Number - or - Upper Bits of ROM Bank Number (Write Only)
// This 2bit register can be used to select a RAM Bank in range from 00-03h, or to specify the upper two bits (Bit 5-6)
// of the ROM Bank number, depending on the current ROM/RAM Mode. (See below.)
//
// 6000-7FFF - ROM/RAM Mode Select (Write Only)
// This 1bit Register selects whether the two bits of the above register should be used as upper two bits of the ROM
// Bank, or as RAM Bank Number.
//   00h = ROM Banking Mode (up to 8KByte RAM, 2MByte ROM) (default)
//   01h = RAM Banking Mode (up to 32KByte RAM, 512KByte ROM)
// The program may freely switch between both modes, the only limitiation is that only RAM Bank 00h can be used during
// Mode 0, and only ROM Banks 00-1Fh can be used during Mode 1.
pub struct Mbc1 {
    rom: Vec<u8>,
    rom_bank: usize,
    rom_maxm: usize,
    ram: Vec<u8>,
    ram_bank: usize,
    ram_maxm: usize,
    ram_open: bool,
    mbc_mode: u8,
    sav_path: PathBuf,
}

impl Mbc1 {
    pub fn power_up(rom: Vec<u8>, ram: Vec<u8>, sav: impl AsRef<Path>) -> Self {
        let rom_maxm = *ROM_BANK_NUMBER.get(&rom[0x0148]).unwrap();
        let ram_maxm = *RAM_BANK_NUMBER.get(&rom[0x0149]).unwrap();
        Mbc1 {
            rom: rom.clone(),
            rom_bank: 0x00,
            rom_maxm,
            ram,
            ram_bank: 0x00,
            ram_maxm,
            ram_open: false,
            mbc_mode: 0x00,
            sav_path: PathBuf::from(sav.as_ref()),
        }
    }
}

impl Memory for Mbc1 {
    fn get(&self, a: u16) -> u8 {
        match a {
            0x0000..=0x3fff => {
                let rom_bank = match self.mbc_mode {
                    0x00 => 0x00,
                    0x01 => 0x00 | self.ram_bank << 5,
                    _ => unreachable!(),
                };
                let rom_bank = rom_bank % self.rom_maxm;
                let bank_off = a as usize & 0x3fff;
                self.rom[rom_bank * 0x4000 + bank_off]
            }
            0x4000..=0x7fff => {
                let rom_bank = self.rom_bank.max(1);
                let rom_bank = match self.mbc_mode {
                    0x00 => rom_bank | self.ram_bank << 5,
                    0x01 => rom_bank,
                    _ => unreachable!(),
                };
                let rom_bank = rom_bank % self.rom_maxm;
                let bank_off = a as usize & 0x3fff;
                self.rom[rom_bank * 0x4000 + bank_off]
            }
            0xa000..=0xbfff => {
                if !self.ram_open {
                    return 0x00;
                }
                let ram_bank = match self.mbc_mode {
                    0x00 => 0x00,
                    0x01 => self.ram_bank,
                    _ => unreachable!(),
                };
                let ram_bank = ram_bank % self.ram_maxm;
                let bank_off = a as usize & 0x1fff;
                self.ram[ram_bank * 0x2000 + bank_off]
            }
            _ => unreachable!(),
        }
    }

    fn set(&mut self, a: u16, v: u8) {
        match a {
            0x0000..=0x1fff => {
                self.ram_open = v & 0x0f == 0x0a;
            }
            0x2000..=0x3fff => {
                self.rom_bank = v as usize & 0x1f;
            }
            0x4000..=0x5fff => {
                self.ram_bank = v as usize & 0x03;
            }
            0x6000..=0x7fff => {
                self.mbc_mode = v;
            }
            0xa000..=0xbfff => {
                if !self.ram_open {
                    return;
                }
                let ram_bank = match self.mbc_mode {
                    0x00 => 0x00,
                    0x01 => self.ram_bank,
                    _ => unreachable!(),
                };
                let ram_bank = ram_bank % self.ram_maxm;
                let bank_off = a as usize & 0x1fff;
                self.ram[ram_bank * 0x2000 + bank_off] = v;
            }
            _ => unreachable!(),
        }
    }
}

impl Stable for Mbc1 {
    fn sav(&self) {
        if self.sav_path.to_str().unwrap().is_empty() {
            return;
        }
        rog::debugln!("Ram is being persisted");
        fs::write(&self.sav_path, &self.ram).unwrap();
    }
}

// 0000-3FFF - ROM Bank 00 (Read Only)
// Same as for MBC1.
//
// 4000-7FFF - ROM Bank 01-0F (Read Only)
// Same as for MBC1, but only a total of 16 ROM banks is supported.
//
// A000-A1FF - 512x4bits RAM, built-in into the MBC2 chip (Read/Write)
// The MBC2 doesn't support external RAM, instead it includes 512x4 bits of built-in RAM (in the MBC2 chip itself). It
// still requires an external battery to save data during power-off though. As the data consists of 4bit values, only
// the lower 4 bits of the "bytes" in this memory area are used.
//
// 0000-1FFF - RAM Enable (Write Only)
// The least significant bit of the upper address byte must be zero to enable/disable cart RAM. For example the
// following addresses can be used to enable/disable cart RAM: 0000-00FF, 0200-02FF, 0400-04FF, ..., 1E00-1EFF.
// The suggested address range to use for MBC2 ram enable/disable is 0000-00FF.
//
// 2000-3FFF - ROM Bank Number (Write Only)
// Writing a value (XXXXBBBB - X = Don't cares, B = bank select bits) into 2000-3FFF area will select an appropriate ROM
// bank at 4000-7FFF.
// The least significant bit of the upper address byte must be one to select a ROM bank. For example the following
// addresses can be used to select a ROM bank: 2100-21FF, 2300-23FF, 2500-25FF, ..., 3F00-3FFF. The suggested address
// range to use for MBC2 rom bank selection is 2100-21FF.
pub struct Mbc2 {
    rom: Vec<u8>,
    rom_bank: usize,
    ram: Vec<u8>,
    ram_open: bool,
    sav_path: PathBuf,
}

impl Mbc2 {
    pub fn power_up(rom: Vec<u8>, ram: Vec<u8>, sav: impl AsRef<Path>) -> Self {
        Self { rom, rom_bank: 0x01, ram, ram_open: false, sav_path: PathBuf::from(sav.as_ref()) }
    }
}

impl Memory for Mbc2 {
    fn get(&self, a: u16) -> u8 {
        match a {
            0x0000..=0x3fff => self.rom[a as usize],
            0x4000..=0x7fff => {
                let rom_bank = self.rom_bank.max(1);
                let rom_bank = rom_bank % 16;
                let bank_off = a as usize & 0x3fff;
                self.rom[rom_bank * 0x4000 + bank_off]
            }
            0xa000..=0xa1ff => {
                if !self.ram_open {
                    return 0x00;
                }
                self.ram[a as usize & 0x01ff]
            }
            0xa200..=0xbfff => {
                if !self.ram_open {
                    return 0x00;
                }
                self.ram[a as usize & 0x01ff]
            }
            _ => unreachable!(),
        }
    }

    fn set(&mut self, a: u16, v: u8) {
        // Only the lower 4 bits of the "bytes" in this memory area are used.
        let v = v & 0x0f;
        match a {
            0x0000..=0x1fff => {
                if a & 0x0100 == 0 {
                    self.ram_open = v == 0x0a;
                }
            }
            0x2000..=0x3fff => {
                if a & 0x0100 != 0 {
                    self.rom_bank = v as usize;
                }
            }
            0xa000..=0xa1ff => {
                if !self.ram_open {
                    return;
                }
                self.ram[a as usize & 0x01ff] = v
            }
            0xa200..=0xbfff => {
                if !self.ram_open {
                    return;
                }
                self.ram[a as usize & 0x01ff] = v
            }
            _ => unreachable!(),
        }
    }
}

impl Stable for Mbc2 {
    fn sav(&self) {
        if self.sav_path.to_str().unwrap().is_empty() {
            return;
        }
        rog::debugln!("Ram is being persisted");
        fs::write(&self.sav_path, &self.ram).unwrap();
    }
}

struct Mbc3Clock {
    s: u8,
    m: u8,
    h: u8,
    d: u8,
    f: u8,
    z: u64,
}

impl Mbc3Clock {
    fn power_up(z: u64) -> Self {
        Self { s: 0, m: 0, h: 0, d: 0, f: 0, z }
    }

    fn latch(&mut self) {
        let el = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() - self.z;
        self.s = (el % 60) as u8;
        self.m = (el / 60 % 60) as u8;
        self.h = (el / 3600 % 24) as u8;
        let ad = (el / 3600 / 24) as u16;
        self.d = (ad % 256) as u8;
        self.f = (self.f & 0xc0) | (ad >> 8) as u8 & 0x01;
        if ad >= 512 {
            self.f |= 0x80;
        }
    }
}

impl Memory for Mbc3Clock {
    fn get(&self, a: u16) -> u8 {
        match a {
            0x08 => self.s,
            0x09 => self.m,
            0x0a => self.h,
            0x0b => self.d,
            0x0c => self.f,
            _ => unreachable!(),
        }
    }

    fn set(&mut self, a: u16, v: u8) {
        match a {
            0x08 => self.s = v,
            0x09 => self.m = v,
            0x0a => self.h = v,
            0x0b => self.d = v,
            0x0c => self.f = v,
            _ => unreachable!(),
        }
        // Recalculate z so the updated registers are reflected on next sav().
        let ad = self.d as u64 | ((self.f as u64 & 0x01) << 8);
        let el = ad * 86400 + self.h as u64 * 3600 + self.m as u64 * 60 + self.s as u64;
        self.z = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs().saturating_sub(el);
    }
}

// Beside for the ability to access up to 2MB ROM (128 banks), and 64KB RAM (8 banks), the MBC3 also includes a
// built-in Real Time Clock (RTC). The RTC requires an external 32.768 kHz Quartz Oscillator, and an external
// battery (if it should continue to tick when the gameboy is turned off).
// 0000-3FFF - ROM Bank 00 (Read Only)
// Same as for MBC1.
//
// 4000-7FFF - ROM Bank 01-7F (Read Only)
// Same as for MBC1, except that accessing banks 20h, 40h, and 60h is supported now.
//
// A000-BFFF - RAM Bank 00-03, if any (Read/Write)
// A000-BFFF - RTC Register 08-0C (Read/Write)
// Depending on the current Bank Number/RTC Register selection (see below), this memory space is used to access an
// 8KByte external RAM Bank, or a single RTC Register.
//
// 0000-1FFF - RAM and Timer Enable (Write Only)
// Mostly the same as for MBC1, a value of 0Ah will enable reading and writing to external RAM - and to the RTC
// Registers! A value of 00h will disable either.
//
// 2000-3FFF - ROM Bank Number (Write Only)
// Same as for MBC1, except that the whole 7 bits of the RAM Bank Number are written directly to this address. As for
// the MBC1, writing a value of 00h, will select Bank 01h instead. All other values 01-7Fh select the corresponding
// ROM Banks.
//
// 4000-5FFF - RAM Bank Number - or - RTC Register Select (Write Only)
// As for the MBC1s RAM Banking Mode, writing a value in range for 00h-07h maps the corresponding external RAM Bank (
// if any) into memory at A000-BFFF. When writing a value of 08h-0Ch, this will map the corresponding RTC register into
// memory at A000-BFFF. That register could then be read/written by accessing any address in that area, typically that
// is done by using address A000.
//
// 6000-7FFF - Latch Clock Data (Write Only)
// When writing 00h, and then 01h to this register, the current time becomes latched into the RTC registers. The
// latched data will not change until it becomes latched again, by repeating the write 00h->01h procedure. This is
// supposed for <reading> from the RTC registers. This can be proven by reading the latched (frozen) time from the RTC
// registers, and then unlatch the registers to show the clock itself continues to tick in background.
//
// The Clock Counter Registers
//  08h  RTC S   Seconds   0-59 (0-3Bh)
//  09h  RTC M   Minutes   0-59 (0-3Bh)
//  0Ah  RTC H   Hours     0-23 (0-17h)
//  0Bh  RTC DL  Lower 8 bits of Day Counter (0-FFh)
//  0Ch  RTC DH  Upper 1 bit of Day Counter, Carry Bit, Halt Flag
//        Bit 0  Most significant bit of Day Counter (Bit 8)
//        Bit 6  Halt (0=Active, 1=Stop Timer)
//        Bit 7  Day Counter Carry Bit (1=Counter Overflow)
// The Halt Flag is supposed to be set before <writing> to the RTC Registers.
//
// The Day Counter
// The total 9 bits of the Day Counter allow to count days in range from 0-511 (0-1FFh). The Day Counter Carry Bit
// becomes set when this value overflows. In that case the Carry Bit remains set until the program does reset it. Note
// that you can store an offset to the Day Counter in battery RAM. For example, every time you read a non-zero Day
// Counter, add this Counter to the offset in RAM, and reset the Counter to zero. This method allows to count any
// number of days, making your program Year-10000-Proof, provided that the cartridge gets used at least every 511 days.
//
// Delays
// When accessing the RTC Registers it is recommended to execute a 4ms delay (4 Cycles in Normal Speed Mode) between
// the separate accesses.
pub struct Mbc3 {
    rom: Vec<u8>,
    rom_bank: usize,
    rom_maxm: usize,
    ram: Vec<u8>,
    ram_bank: usize,
    ram_maxm: usize,
    ram_open: bool,
    rtc: Mbc3Clock,
    sav_path: PathBuf,
    rtc_path: PathBuf,
}

impl Mbc3 {
    pub fn power_up(rom: Vec<u8>, ram: Vec<u8>, sav: impl AsRef<Path>, rtc: impl AsRef<Path>) -> Self {
        let rom_maxm = *ROM_BANK_NUMBER.get(&rom[0x0148]).unwrap();
        let ram_maxm = *RAM_BANK_NUMBER.get(&rom[0x0149]).unwrap();
        let rtc_zero = match fs::read(&rtc) {
            Ok(ok) => u64::from_be_bytes(ok.try_into().unwrap()),
            Err(_) => SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs(),
        };
        Self {
            rom,
            rom_bank: 0,
            rom_maxm,
            ram,
            ram_bank: 0,
            ram_maxm,
            ram_open: false,
            rtc: Mbc3Clock::power_up(rtc_zero),
            sav_path: PathBuf::from(sav.as_ref()),
            rtc_path: PathBuf::from(rtc.as_ref()),
        }
    }
}

impl Memory for Mbc3 {
    fn get(&self, a: u16) -> u8 {
        match a {
            0x0000..=0x3fff => self.rom[a as usize],
            0x4000..=0x7fff => {
                let rom_bank = self.rom_bank.max(1);
                let rom_bank = rom_bank % self.rom_maxm;
                let bank_off = a as usize & 0x3fff;
                self.rom[rom_bank * 0x4000 + bank_off]
            }
            0xa000..=0xbfff => {
                if !self.ram_open {
                    return 0x00;
                }
                if self.ram_bank <= 0x07 {
                    let ram_bank = self.ram_bank % self.ram_maxm;
                    let bank_off = a as usize & 0x1fff;
                    return self.ram[ram_bank * 0x2000 + bank_off];
                }
                self.rtc.get(self.ram_bank as u16)
            }
            _ => unreachable!(),
        }
    }

    fn set(&mut self, a: u16, v: u8) {
        match a {
            0x0000..=0x1fff => {
                self.ram_open = v & 0x0f == 0x0a;
            }
            0x2000..=0x3fff => {
                self.rom_bank = v as usize & 0x7f;
            }
            0x4000..=0x5fff => {
                self.ram_bank = v as usize & 0x0f;
            }
            0x6000..=0x7fff => {
                if v & 0x01 != 0 {
                    self.rtc.latch();
                }
            }
            0xa000..=0xbfff => {
                if !self.ram_open {
                    return;
                }
                if self.ram_bank <= 0x07 {
                    let ram_bank = self.ram_bank % self.ram_maxm;
                    let bank_off = a as usize & 0x1fff;
                    self.ram[ram_bank * 0x2000 + bank_off] = v;
                    return;
                }
                self.rtc.set(self.ram_bank as u16, v)
            }
            _ => unreachable!(),
        }
    }
}

impl Stable for Mbc3 {
    fn sav(&self) {
        if self.sav_path.to_str().unwrap().is_empty() {
            return;
        }
        rog::debugln!("Ram is being persisted");
        fs::write(&self.sav_path, &self.ram).unwrap();
        if self.rtc_path.to_str().unwrap().is_empty() {
            return;
        }
        fs::write(&self.rtc_path, &self.rtc.z.to_be_bytes()).unwrap();
    }
}

pub struct Mbc5 {
    rom: Vec<u8>,
    rom_bank: usize,
    rom_maxm: usize,
    ram: Vec<u8>,
    ram_bank: usize,
    ram_maxm: usize,
    ram_open: bool,
    sav_path: PathBuf,
}

impl Mbc5 {
    pub fn power_up(rom: Vec<u8>, ram: Vec<u8>, sav: impl AsRef<Path>) -> Self {
        let rom_maxm = *ROM_BANK_NUMBER.get(&rom[0x0148]).unwrap();
        let ram_maxm = *RAM_BANK_NUMBER.get(&rom[0x0149]).unwrap();
        Self {
            rom,
            rom_bank: 1,
            rom_maxm,
            ram,
            ram_bank: 0,
            ram_maxm,
            ram_open: false,
            sav_path: PathBuf::from(sav.as_ref()),
        }
    }
}

impl Memory for Mbc5 {
    fn get(&self, a: u16) -> u8 {
        match a {
            0x0000..=0x3fff => self.rom[a as usize],
            0x4000..=0x7fff => {
                let rom_bank = self.rom_bank % self.rom_maxm;
                let bank_off = a as usize & 0x3fff;
                self.rom[rom_bank * 0x4000 + bank_off]
            }
            0xa000..=0xbfff => {
                if !self.ram_open {
                    return 0x00;
                }
                let ram_bank = self.ram_bank % self.ram_maxm;
                let bank_off = a as usize & 0x1fff;
                self.ram[ram_bank * 0x2000 + bank_off]
            }
            _ => unreachable!(),
        }
    }

    fn set(&mut self, a: u16, v: u8) {
        match a {
            0x0000..=0x1fff => {
                self.ram_open = v & 0x0f == 0x0a;
            }
            0x2000..=0x2fff => self.rom_bank = (self.rom_bank & 0x100) | (((v & 0xff) as usize) << 0),
            0x3000..=0x3fff => self.rom_bank = (self.rom_bank & 0x0ff) | (((v & 0x01) as usize) << 8),
            0x4000..=0x5fff => self.ram_bank = (v & 0x0f) as usize,
            0xa000..=0xbfff => {
                if !self.ram_open {
                    return;
                }
                let ram_bank = self.ram_bank % self.ram_maxm;
                let bank_off = a as usize & 0x1fff;
                self.ram[ram_bank * 0x2000 + bank_off] = v;
            }
            _ => unreachable!(),
        }
    }
}

impl Stable for Mbc5 {
    fn sav(&self) {
        if self.sav_path.to_str().unwrap().is_empty() {
            return;
        }
        rog::debugln!("Ram is being persisted");
        fs::write(&self.sav_path, &self.ram).unwrap();
    }
}

// HuC1 is an MBC developed by Hudson Soft. It implements ROM and RAM banking, and also provides infrared
// communication. Despite many sources on the internet claiming that HuC1 is "similar to MBC1", it actually differs
// from MBC1 significantly.
// The Japanese cart "Fighting Phoenix" (internal cart name: SUPER B DAMAN) is known to contain this chip.
//
// 0000-1FFF - IR Select (Write Only)
// Most MBCs can disable the cartridge RAM to prevent accidental writes. HuC1 doesn't do this. Instead, this register
// switches the A000-BFFF region between "RAM mode" and "IR mode". Write $0E to switch to IR mode, or anything else
// to switch to RAM mode.
//
// 2000-3FFF - ROM Bank Number (Write Only)
// HuC1 can accept a bank number of at least 6 bits here.
//
// 4000-5FFF - RAM Bank Select (Write Only)
// HuC1 can accept a bank number of at least 2 bits here.
//
// 6000-7FFF - Nothing (Write Only)
// Writes to this region seem to have no effect.
//
// A000-BFFF - Cart RAM or IR register (Read/Write)
// When in IR mode (wrote $0E to $0000), the IR register is visible here. Write to this region to control the IR
// transmitter ($01 = on, $00 = off). Read from this region to see either $C1 (saw light) or $C0 (did not see light).
// When in RAM mode (wrote something other than $0E to $0000) this region behaves like normal cart RAM.
pub struct HuC1 {
    rom: Vec<u8>,
    rom_bank: usize,
    rom_maxm: usize,
    ram: Vec<u8>,
    ram_bank: usize,
    ram_maxm: usize,
    ram_open: bool,
    sav_path: PathBuf,
}

impl HuC1 {
    pub fn power_up(rom: Vec<u8>, ram: Vec<u8>, sav: impl AsRef<Path>) -> Self {
        let rom_maxm = *ROM_BANK_NUMBER.get(&rom[0x0148]).unwrap();
        let ram_maxm = *RAM_BANK_NUMBER.get(&rom[0x0149]).unwrap();
        Self {
            rom,
            rom_bank: 0x01,
            rom_maxm,
            ram,
            ram_bank: 0x00,
            ram_maxm,
            ram_open: true,
            sav_path: PathBuf::from(sav.as_ref()),
        }
    }
}

impl Memory for HuC1 {
    fn get(&self, a: u16) -> u8 {
        match a {
            0x0000..=0x3fff => self.rom[a as usize],
            0x4000..=0x7fff => {
                let rom_bank = self.rom_bank.max(1);
                let rom_bank = rom_bank % self.rom_maxm;
                let bank_off = a as usize & 0x3fff;
                self.rom[rom_bank * 0x4000 + bank_off]
            }
            0xa000..=0xbfff => {
                if !self.ram_open {
                    return 0xc0;
                }
                let ram_bank = self.ram_bank % self.ram_maxm;
                let bank_off = a as usize & 0x1fff;
                self.ram[ram_bank * 0x2000 + bank_off]
            }
            _ => unreachable!(),
        }
    }

    fn set(&mut self, a: u16, v: u8) {
        match a {
            0x0000..=0x1fff => {
                self.ram_open = v != 0x0e;
            }
            0x2000..=0x3fff => {
                self.rom_bank = v as usize & 0x3f;
            }
            0x4000..=0x5fff => {
                self.ram_bank = v as usize & 0x03;
            }
            0x6000..=0x7fff => {}
            0xa000..=0xbfff => {
                if !self.ram_open {
                    return;
                }
                let ram_bank = self.ram_bank % self.ram_maxm;
                let bank_off = a as usize & 0x1fff;
                self.ram[ram_bank * 0x2000 + bank_off] = v;
            }
            _ => unreachable!(),
        }
    }
}

impl Stable for HuC1 {
    fn sav(&self) {
        if self.sav_path.to_str().unwrap().is_empty() {
            return;
        }
        rog::debugln!("Ram is being persisted");
        fs::write(&self.sav_path, &self.ram).unwrap();
    }
}

pub struct Cartridge {
    pub inner: Box<dyn Stable>,
    pub title: String,
}

impl Cartridge {
    // Specifies which Memory Bank Controller (if any) is used in the cartridge, and if further external hardware exists
    // in the cartridge.
    pub fn power_up(path: impl AsRef<Path>) -> Self {
        rog::debugln!("Loading cartridge from {:?}", path.as_ref());
        let rom = fs::read(&path).unwrap();
        assert!(rom.len() >= 0x150, "Missing required information area which located at 0100-014F");
        assert!(rom[0x0104..0x0134] == NINTENDO_LOGO, "Nintendo logo is not correct");
        let rom_max = ROM_BANK_NUMBER.get(&rom[0x0148]).unwrap() * ROM_BANK_LENGTH;
        assert!(rom.len() <= rom_max, "Rom size more than {}", rom_max);
        let mut title = String::new();
        for i in 0x0134..if rom[0x0143] & 0x80 != 0 { 0x013f } else { 0x0144 } {
            match rom[i] {
                0 => break,
                v => title.push(v as char),
            }
        }
        rog::debugln!("Cartridge name is {}", title);
        rog::debugln!("Cartridge type is {}", READABLE_TYPE.get(&rom[0x0147]).unwrap());
        let mut check: u8 = 0;
        for i in 0x0134..0x014d {
            check = check.wrapping_sub(rom[i]).wrapping_sub(1);
        }
        assert!(rom[0x014d] == check, "Cartridge's header checksum is incorrect");
        let mut inner: Box<dyn Stable> = match rom[0x0147] {
            0x00 => Box::new(RomOnly::power_up(rom)),
            0x01 => Box::new(Mbc1::power_up(rom, vec![], "")),
            0x02 => {
                let ram_size = RAM_BANK_NUMBER.get(&rom[0x0149]).unwrap() * RAM_BANK_LENGTH;
                let ram = vec![0; ram_size];
                Box::new(Mbc1::power_up(rom, ram, ""))
            }
            0x03 => {
                let ram_size = RAM_BANK_NUMBER.get(&rom[0x0149]).unwrap() * RAM_BANK_LENGTH;
                let sav_path = path.as_ref().to_path_buf().with_extension("sav");
                let ram = fs::read(&sav_path).unwrap_or_else(|_| vec![0; ram_size]);
                Box::new(Mbc1::power_up(rom, ram, sav_path))
            }
            0x05 => {
                let ram_size = 512;
                let ram = vec![0; ram_size];
                Box::new(Mbc2::power_up(rom, ram, ""))
            }
            0x06 => {
                let ram_size = 512;
                let sav_path = path.as_ref().to_path_buf().with_extension("sav");
                let ram = fs::read(&sav_path).unwrap_or_else(|_| vec![0; ram_size]);
                Box::new(Mbc2::power_up(rom, ram, sav_path))
            }
            0x0f => {
                let sav_path = path.as_ref().to_path_buf().with_extension("sav");
                let rtc_path = path.as_ref().to_path_buf().with_extension("rtc");
                Box::new(Mbc3::power_up(rom, vec![], sav_path, rtc_path))
            }
            0x10 => {
                let ram_size = RAM_BANK_NUMBER.get(&rom[0x0149]).unwrap() * RAM_BANK_LENGTH;
                let sav_path = path.as_ref().to_path_buf().with_extension("sav");
                let rtc_path = path.as_ref().to_path_buf().with_extension("rtc");
                let ram = fs::read(&sav_path).unwrap_or_else(|_| vec![0; ram_size]);
                Box::new(Mbc3::power_up(rom, ram, sav_path, rtc_path))
            }
            0x11 => Box::new(Mbc3::power_up(rom, vec![], "", "")),
            0x12 => {
                let ram_size = RAM_BANK_NUMBER.get(&rom[0x0149]).unwrap() * RAM_BANK_LENGTH;
                let ram = vec![0; ram_size];
                Box::new(Mbc3::power_up(rom, ram, "", ""))
            }
            0x13 => {
                let ram_size = RAM_BANK_NUMBER.get(&rom[0x0149]).unwrap() * RAM_BANK_LENGTH;
                let sav_path = path.as_ref().to_path_buf().with_extension("sav");
                let ram = fs::read(&sav_path).unwrap_or_else(|_| vec![0; ram_size]);
                Box::new(Mbc3::power_up(rom, ram, sav_path, ""))
            }
            0x19 => Box::new(Mbc5::power_up(rom, vec![], "")),
            0x1a => {
                let ram_size = RAM_BANK_NUMBER.get(&rom[0x0149]).unwrap() * RAM_BANK_LENGTH;
                let ram = vec![0; ram_size];
                Box::new(Mbc5::power_up(rom, ram, ""))
            }
            0x1b => {
                let ram_size = RAM_BANK_NUMBER.get(&rom[0x0149]).unwrap() * RAM_BANK_LENGTH;
                let sav_path = path.as_ref().to_path_buf().with_extension("sav");
                let ram = fs::read(&sav_path).unwrap_or_else(|_| vec![0; ram_size]);
                Box::new(Mbc5::power_up(rom, ram, sav_path))
            }
            0xff => {
                let ram_size = RAM_BANK_NUMBER.get(&rom[0x0149]).unwrap() * RAM_BANK_LENGTH;
                let sav_path = path.as_ref().to_path_buf().with_extension("sav");
                let ram = fs::read(&sav_path).unwrap_or_else(|_| vec![0; ram_size]);
                Box::new(HuC1::power_up(rom, ram, sav_path))
            }
            _ => unreachable!(),
        };
        let _ = inner.as_mut();
        Self { inner, title }
    }
}

impl Memory for Cartridge {
    fn get(&self, a: u16) -> u8 {
        self.inner.get(a)
    }

    fn set(&mut self, a: u16, v: u8) {
        self.inner.set(a, v);
    }
}

impl Stable for Cartridge {
    fn sav(&self) {
        self.inner.sav();
    }
}
